use lode_original::session::{Configuration as OriginalConfiguration, Run as OriginalRun};
use lode_port::devices::speaker::Speaker;
use lode_port::event_timing::{Model, TimingInterval};
use lode_port::graphics::{
    compute_shifted_sprite, draw_sprite, draw_sprite_at_pixels, erase_sprite_at_pixels,
};
use lode_port::session::{Configuration, Run};
use lode_port::sound::play_note;
use lode_port::storage::{LevelRecord, original_level};
use lode_port::timing::Clock;
use lode_trace::Phase;

// Input-only execution protects gameplay across timing changes. Complete model
// turns must retain the previously calibrated duration on its gameplay domain.
#[test]
fn all_boards_match_original_gameplay_and_calibrated_turn_costs() {
    let old_model = lode_original::event_timing::Model::calibrated();
    for level in 1..=150 {
        let config = Configuration {
            high_scores: vec![],
            levels: vec![LevelRecord(original_level(level).unwrap())],
            start: 0,
            wait_on_level_start: false,
        };
        let old_config: OriginalConfiguration =
            serde_json::from_str(&serde_json::to_string(&config).unwrap()).unwrap();
        let mut old = OriginalRun::new(old_config, false).unwrap();
        let mut new = Run::new(config, false).unwrap();
        for turn in 0..90 {
            let key = match turn {
                0 => Some(0xcc),
                11 => Some(0xa0),
                20 => Some(0xca),
                31 => Some(0xd5),
                45 => Some(0xcf),
                60 => Some(0xc9),
                75 => Some(0xcb),
                _ => None,
            };
            old.step(key).unwrap();
            new.step(key).unwrap();
            assert_eq!(
                new.game.snapshot(),
                old.game.snapshot(),
                "level {level}, turn {turn}"
            );
            let previous = old.last_timing().unwrap();
            if previous.phase == Phase::Playing
                && matches!(
                    previous.end_phase,
                    Phase::Playing | Phase::Died | Phase::LevelCleared
                )
            {
                assert_eq!(
                    new.last_model_cycles().unwrap(),
                    old_model.estimate(previous),
                    "level {level}, turn {turn}"
                );
            }
        }
        let expected = old.trace().unwrap();
        let mut actual = new.trace().unwrap();
        for (actual, expected) in actual.rounds.iter_mut().zip(&expected.rounds) {
            actual.start_delta = expected.start_delta;
            assert_eq!(actual, expected, "level {level}: logical round");
        }
    }
}

// Graphics simplifications must preserve every packed byte and collision result,
// including all shift alignments, both widths and the status row's table crossings.
#[test]
fn sprite_shifts_and_draw_erase_match_original_packed_graphics() {
    for sprite in 0..104 {
        for shift in 0..7 {
            assert_eq!(
                compute_shifted_sprite(sprite, shift),
                lode_original::graphics::compute_shifted_sprite(sprite, shift)
            );
        }
    }
    let mut old = lode_original::devices::hires::Hires::default();
    let mut new = lode_port::devices::hires::Hires::default();
    for (sprite, col, row) in [
        (0, 0, 0),
        (9, 8, 5),
        (37, 27, 15),
        (68, 5, 16),
        (103, 26, 11),
    ] {
        draw_sprite(&mut new, 1, sprite, col, row);
        lode_original::graphics::draw_sprite(&mut old, 1, sprite, col, row);
        assert_eq!(
            draw_sprite_at_pixels(&mut new, sprite, col * 5, row * 11),
            lode_original::graphics::draw_sprite_at_pixels(&mut old, sprite, col * 5, row * 11)
        );
        erase_sprite_at_pixels(&mut new, sprite, col * 5, row * 11);
        lode_original::graphics::erase_sprite_at_pixels(&mut old, sprite, col * 5, row * 11);
    }
    for address in 0x2000..0x6000 {
        assert_eq!(new.read(address), old.read(address));
    }
}

#[test]
fn centralized_notes_preserve_speaker_waveforms_and_counter_wraps() {
    for (pitch, duration, low) in [
        (20, 1, 1),
        (20, 1, 0),
        (0, 2, 255),
        (1, 8, 0),
        (100, 8, 15),
        (40, 0, 2),
    ] {
        for enabled in [true, false] {
            let mut old_clock = lode_original::timing::Clock::new(false);
            let mut old_speaker = lode_original::devices::speaker::Speaker::default();
            lode_original::sound::play_note(
                &mut old_clock,
                &mut old_speaker,
                enabled,
                pitch,
                duration,
                low,
            );
            let mut clock = Clock::new(false);
            let mut speaker = Speaker::default();
            play_note(&mut clock, enabled, pitch, duration, low);
            let interval = TimingInterval {
                phase: Phase::Playing,
                end_phase: Phase::Playing,
                events: std::mem::take(&mut clock.events),
            };
            clock.commit(&Model::calibrated(), &interval, &mut speaker);
            assert_eq!(clock.cycles, old_clock.cycles);
            assert_eq!(speaker.edges.len(), old_speaker.edges.len());
            for (actual, expected) in speaker.edges.iter().zip(&old_speaker.edges) {
                assert_eq!((actual.cycle, actual.high), (expected.cycle, expected.high));
            }
        }
    }
}
