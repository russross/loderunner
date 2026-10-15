use lode_original::devices::hires::Hires;
use lode_original::graphics::{draw_sprite, draw_sprite_at_pixels, erase_sprite_at_pixels};
use lode_original::session::{Configuration as OriginalConfiguration, Run as OriginalRun};
use lode_port::collision::{Layer, Scene};
use lode_port::devices::speaker::Speaker;
use lode_port::event_timing::{Draw, DrawKind, Events, Model, TimingEvent, TimingInterval};
use lode_port::graphics::sprite_rows;
use lode_port::session::{Configuration, Run};
use lode_port::sound::play_note;
use lode_port::storage::{LevelRecord, original_level};
use lode_port::timing::Clock;
use lode_trace::Phase;
use std::collections::BTreeMap;

mod support;

// Whole-sprite costs preserve the reference's alignment and page-boundary
// effects without executing per-plane or per-scanline loops in the model.
#[test]
fn reference_draw_costs_cover_alignment_and_vertical_boundaries() {
    let model = Model {
        coefficients: BTreeMap::from([(TimingEvent::GraphicsCycles, 1.0)]),
    };
    let mut original = Hires::default();
    let estimate = |draw| {
        let mut events = Events::default();
        events.draw(draw);
        model.estimate(&TimingInterval {
            phase: Phase::Playing,
            end_phase: Phase::Playing,
            events,
        })
    };
    for sprite in 0..102 {
        for half_x in [0, 1, 2, 3, 4, 5, 6, 101, 102, 103, 136, 139] {
            for y in [0, 112, 113, 122, 123, 176, 177, 181] {
                draw_sprite_at_pixels(&mut original, sprite, half_x, y);
                assert_eq!(
                    estimate(Draw {
                        kind: DrawKind::Actor,
                        sprite,
                        half_x,
                        y
                    }),
                    original.take_cycles()
                );
                erase_sprite_at_pixels(&mut original, sprite, half_x, y);
                assert_eq!(
                    estimate(Draw {
                        kind: DrawKind::Erase,
                        sprite,
                        half_x,
                        y
                    }),
                    original.take_cycles()
                );
            }
        }
        // Tile placement uses its own cost and the status row's lower baseline.
        for col in 0..28 {
            for row in [0, 10, 11, 15, 16] {
                draw_sprite(&mut original, 0, sprite, col, row);
                assert_eq!(
                    estimate(Draw {
                        kind: DrawKind::Tile,
                        sprite,
                        half_x: col * 5,
                        y: if row == 16 { 181 } else { row * 11 }
                    }),
                    original.take_cycles()
                );
            }
        }
    }
}

// Independent gameplay execution must produce the same workload for the native
// model, even though the baseline also collects unused branch-level features.
#[test]
fn all_boards_match_original_gameplay_and_calibrated_turn_costs() {
    let old_model = support::reference_model();
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
            support::assert_visible(&new.game.scene, &old.game.hires);
            new.game.display.prepare(&new.game.scene);
            new.game.display.acknowledge(&mut new.game.scene);
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

// Cached sprite geometry must preserve every original placement and color phase,
// including seven shifts, arbitrary overlays, and the status area's placement.
#[test]
fn sprite_geometry_and_composition_match_original_visible_graphics() {
    for sprite in 0..102 {
        for shift in 0..7 {
            let shifted = lode_original::graphics::compute_shifted_sprite(sprite, shift);
            for (row, expected) in sprite_rows(sprite).into_iter().enumerate() {
                let bytes = &shifted[row * 3..row * 3 + 3];
                let pixels = u32::from(bytes[0] & 127)
                    | u32::from(bytes[1] & 127) << 7
                    | u32::from(bytes[2] & 127) << 14;
                assert_eq!(pixels >> shift, u32::from(expected));
                assert!(
                    bytes
                        .iter()
                        .take(if shift >= 5 { 3 } else { 2 })
                        .all(|byte| byte & 128 != 0)
                );
            }
        }
    }
    let mut old = lode_original::devices::hires::Hires::default();
    let mut new = Scene::default();
    for (sprite, col, row) in [
        (0, 0, 0),
        (9, 8, 5),
        (37, 27, 15),
        (68, 5, 16),
        (101, 26, 11),
    ] {
        new.tile(Layer::Background, sprite, col, row);
        lode_original::graphics::draw_sprite(&mut old, 1, sprite, col, row);
        new.overlay(sprite, col * 5, row * 11);
        lode_original::graphics::draw_sprite_at_pixels(&mut old, sprite, col * 5, row * 11);
        support::assert_visible(&new, &old);
        new.erase(sprite, col * 5, row * 11);
        lode_original::graphics::erase_sprite_at_pixels(&mut old, sprite, col * 5, row * 11);
    }
    support::assert_visible(&new, &old);
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

fn board(tiles: &[(usize, usize, lode_trace::Tile)]) -> LevelRecord {
    let mut bytes = [0; 256];
    for &(col, row, tile) in tiles {
        let index = row * 28 + col;
        bytes[index / 2] |= (tile as u8) << ((index % 2) * 4);
    }
    LevelRecord(bytes)
}

// A custom board can exhaust the byte-sized gold count. Exit eligibility and
// collection after wrapping must still agree with original gameplay.
#[test]
fn crowded_gold_boards_preserve_exit_and_collection_behavior() {
    use lode_trace::Tile;

    for (count, player) in [254, 255, 256, 446]
        .into_iter()
        .flat_map(|count| [(count, 0), (count, 420)])
    {
        let mut tiles = vec![
            (player % 28, player / 28, Tile::Player),
            (0, 1, Tile::Brick),
        ];
        tiles.extend(
            (1..448)
                .map(|offset| (player + offset) % 448)
                .filter(|&index| index != 28)
                .take(count)
                .map(|index| (index % 28, index / 28, Tile::Gold)),
        );
        let config = Configuration {
            levels: vec![board(&tiles)],
            start: 0,
            high_scores: vec![],
            wait_on_level_start: false,
        };
        let old_config = serde_json::from_str(&serde_json::to_string(&config).unwrap()).unwrap();
        let mut old = OriginalRun::new(old_config, false).unwrap();
        let mut new = Run::new(config, false).unwrap();
        for key in [Some(0xcc), None, None, None, None, None] {
            old.step(key).unwrap();
            new.step(key).unwrap();
            assert_eq!(
                new.game.snapshot(),
                old.game.snapshot(),
                "gold count {count}"
            );
            support::assert_visible(&new.game.scene, &old.game.hires);
        }
    }
}

// These scenarios cross lifecycle boundaries that ordinary movement does not:
// guard burial, hidden resurrection stages, dormant facing slots and menus.
#[test]
fn lifecycle_and_digging_preserve_complete_frames_and_occurrences() {
    use lode_trace::Tile;
    let mut floor: Vec<_> = (0..28).map(|col| (col, 15, Tile::Brick)).collect();
    floor.extend([
        (5, 14, Tile::Player),
        (27, 14, Tile::Guard),
        (25, 10, Tile::Gold),
    ]);
    let mut burial_keys = vec![None; 250];
    burial_keys[0] = Some(0xcf);
    burial_keys[13] = Some(0xa0);
    let many = board(&[
        (0, 15, Tile::Player),
        (5, 15, Tile::Guard),
        (10, 15, Tile::Guard),
        (15, 15, Tile::Guard),
        (20, 15, Tile::Guard),
        (25, 15, Tile::Guard),
        (26, 14, Tile::Gold),
    ]);
    let few = board(&[
        (0, 15, Tile::Player),
        (5, 15, Tile::Guard),
        (26, 14, Tile::Gold),
    ]);
    let mut reload_keys = vec![None; 60];
    for index in [15, 31, 47] {
        reload_keys[index] = Some(0x9e);
    }
    let mut scenarios = vec![
        (vec![board(&floor)], burial_keys),
        (vec![many.clone(), few, many], reload_keys),
        (
            vec![board(&[(5, 15, Tile::Player), (25, 15, Tile::Gold)])],
            vec![
                Some(0x9b),
                None,
                Some(0x9b),
                Some(0x8d),
                None,
                Some(0xa0),
                Some(0x81),
                None,
            ],
        ),
    ];
    for side in [0xcf, 0xd5] {
        let mut keys = vec![None; 210];
        keys[0] = Some(side);
        keys[13] = Some(0xa0);
        scenarios.push((
            vec![board(&[
                (5, 14, Tile::Player),
                (4, 15, Tile::Brick),
                (5, 15, Tile::Brick),
                (6, 15, Tile::Brick),
            ])],
            keys,
        ));
    }
    for (levels, keys) in scenarios {
        let config = Configuration {
            levels,
            start: 0,
            high_scores: vec![],
            wait_on_level_start: false,
        };
        let old_config = serde_json::from_str(&serde_json::to_string(&config).unwrap()).unwrap();
        let mut old = OriginalRun::new(old_config, false).unwrap();
        let mut new = Run::new(config, false).unwrap();
        for (turn, key) in keys.into_iter().enumerate() {
            old.step(key).unwrap();
            new.step(key).unwrap();
            assert_eq!(
                new.game.snapshot(),
                old.game.snapshot(),
                "lifecycle turn {turn}"
            );
            support::assert_visible(&new.game.scene, &old.game.hires);
            new.game.display.prepare(&new.game.scene);
            for row in 0..192 {
                assert_eq!(
                    &new.game.display.pixels()[row * 840..(row + 1) * 840],
                    old.game.hires.rgb_row(row),
                    "selected page row {row}"
                );
            }
            new.game.display.acknowledge(&mut new.game.scene);
        }
        let old_trace = old.trace().unwrap();
        let mut new_trace = new.trace().unwrap();
        for (actual, expected) in new_trace.rounds.iter_mut().zip(&old_trace.rounds) {
            actual.start_delta = expected.start_delta;
            assert_eq!(actual, expected);
        }
    }
}
