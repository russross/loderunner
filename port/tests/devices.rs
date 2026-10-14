use lode_port::devices::hires::Hires;
use lode_port::devices::keyboard::Keyboard;
use lode_port::devices::speaker::Speaker;
use lode_port::event_timing::{Model, TimingInterval};
use lode_port::graphics::{draw_sprite_at_pixels, erase_sprite_at_pixels, row_address};
use lode_port::sound::{Note, Sound, play_note};
use lode_port::timing::Clock;
use lode_trace::Phase;

#[test]
fn hires_holes_persist_without_dirty_pixels_and_page_switch_invalidates() {
    let mut hires = Hires::default();
    hires.acknowledge();
    hires.write(0x2078, 0x42);
    assert_eq!(hires.read(0x2078), 0x42);
    assert!(hires.dirty_rows().is_empty());
    hires.write(row_address(0, 80), 0x81);
    assert_eq!(hires.dirty_rows(), vec![80]);
    hires.select(1);
    assert_eq!(hires.dirty_rows().len(), 192);
}

#[test]
fn original_scanline_tables_match_visible_address_mapping() {
    for row in 0..192 {
        assert_eq!(
            row_address(0, row as u8),
            0x2000 + Hires::row_offset(row) as u16
        );
        assert_eq!(
            row_address(1, row as u8),
            0x4000 + Hires::row_offset(row) as u16
        );
    }
}

#[test]
fn keyboard_acknowledgement_retains_code_and_repeated_delivery_is_available() {
    let mut keyboard = Keyboard::default();
    keyboard.deliver(0xcc);
    assert_eq!(keyboard.read_mmio(0xc000), 0xcc);
    keyboard.write_mmio(0xc010, 0);
    assert_eq!(keyboard.read_mmio(0xc000), 0x4c);
    keyboard.deliver(0xcc);
    assert_eq!(keyboard.read_mmio(0xc000), 0xcc);
    keyboard.read_mmio(0xc010);
    assert_eq!(keyboard.read(), 0x4c);
}

#[test]
fn collision_is_reported_on_repeated_draw_and_erasure_restores_background() {
    let mut hires = Hires::default();
    assert_eq!(draw_sprite_at_pixels(&mut hires, 9, 50, 55), 0);
    assert_ne!(draw_sprite_at_pixels(&mut hires, 9, 50, 55), 0);
    erase_sprite_at_pixels(&mut hires, 9, 50, 55);
    assert_eq!(draw_sprite_at_pixels(&mut hires, 9, 50, 55), 0);
}

#[test]
fn speaker_averages_edges_inside_samples() {
    let mut speaker = Speaker::default();
    speaker.access(2);
    speaker.access(6);
    assert_eq!(speaker.samples(0, 8, 1, 8), vec![0.0]);
}

#[test]
fn audio_batch_boundaries_and_pruning_preserve_sample_sequence() {
    let mut speaker = Speaker::default();
    for cycle in [2, 7, 13, 23, 41, 70] {
        speaker.access(cycle);
    }
    let expected = speaker.sample_frames(0, 20, 7, 31);
    let mut batches = speaker.sample_frames(0, 8, 7, 31);
    speaker.discard_before(8 * 31 / 7);
    batches.extend(speaker.sample_frames(8, 12, 7, 31));
    for (actual, expected) in batches.iter().zip(expected) {
        assert!((actual - expected).abs() < 0.00001);
    }
}

#[test]
fn short_note_uses_incoming_counter_and_muting_keeps_delay() {
    let mut audible = Clock::new(false);
    let mut speaker = Speaker::default();
    play_note(&mut audible, true, 20, 1, 1);
    assert_eq!(audible.cycles, 0);
    assert!(speaker.edges.is_empty());
    let interval = TimingInterval {
        phase: Phase::Playing,
        end_phase: Phase::Playing,
        events: std::mem::take(&mut audible.events),
    };
    audible.commit(&Model::calibrated(), &interval, &mut speaker);
    assert_eq!(speaker.edges.len(), 1);
    assert_eq!(speaker.edges[0].cycle, 15);
    assert_eq!(audible.cycles, 33);
    let mut muted = Clock::new(false);
    let mut silent = Speaker::default();
    play_note(&mut muted, false, 20, 1, 1);
    let mut interval = TimingInterval {
        phase: Phase::Playing,
        end_phase: Phase::Playing,
        events: std::mem::take(&mut muted.events),
    };
    muted.commit(&Model::calibrated(), &interval, &mut silent);
    assert!(silent.edges.is_empty());
    assert_eq!(muted.cycles, 30);
    play_note(&mut muted, false, 20, 1, 0);
    interval.events = std::mem::take(&mut muted.events);
    muted.commit(&Model::calibrated(), &interval, &mut silent);
    assert!(muted.cycles > 2500);
}

#[test]
fn sound_loading_appends_and_playback_index_selects_last_note() {
    let mut sound = Sound::default();
    sound
        .load_sound_data(&[
            Note {
                duration: 7,
                pitch: 0x45,
            },
            Note {
                duration: 6,
                pitch: 0x55,
            },
        ])
        .unwrap();
    assert_eq!(sound.index, 2);
    assert_eq!(sound.pitches[usize::from(sound.index)], 0x55);
    assert!(
        sound
            .load_sound_data(
                &[Note {
                    duration: 1,
                    pitch: 1
                }; 126]
            )
            .is_err()
    );
    assert_eq!(sound.index, 2);
}
