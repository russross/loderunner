use lode_trace::{Phase, Tile};
use loderunner::recording::Recording;
use loderunner::session::{Configuration, Run};
use loderunner::storage::LevelRecord;

fn configuration(tiles: &[(usize, usize, Tile)]) -> Configuration {
    let mut bytes = [0; 256];
    for &(col, row, tile) in tiles {
        let index = row * 28 + col;
        bytes[index / 2] |= (tile as u8) << ((index % 2) * 4);
    }
    Configuration {
        levels: vec![LevelRecord(bytes)],
        start: 0,
        wait_on_level_start: false,
    }
}

// Capture consumed keys, then rebuild gameplay independently at those boundaries.
fn round_trip(configuration: Configuration, deliveries: &[Option<u8>]) -> Recording {
    let mut recording = Recording::new(configuration.clone());
    let mut original = Run::new(configuration, false).unwrap();
    for &delivery in deliveries {
        let transition = matches!(
            original.game.session.phase,
            Phase::Died | Phase::LevelCleared
        );
        original.step(delivery).unwrap();
        if !transition {
            recording.push(original.last_inputs()).unwrap();
        }
    }
    let recording = Recording::from_json(&recording.to_json().unwrap()).unwrap();
    let mut playback = Run::new(recording.configuration.clone(), false).unwrap();
    for round in 0..recording.rounds {
        if matches!(
            playback.game.session.phase,
            Phase::Died | Phase::LevelCleared
        ) {
            playback.step(None).unwrap();
        }
        playback.start_ready_level();
        playback.step(recording.key(round)).unwrap();
    }
    assert_eq!(original.trace().unwrap(), playback.trace().unwrap());
    recording
}

#[test]
fn falling_records_input_when_observed_and_preserves_the_endpoint() {
    let config = configuration(&[(5, 13, Tile::Player), (5, 15, Tile::Brick)]);
    let mut deliveries = vec![None; 16];
    deliveries[0] = Some(0xcc);
    let recording = round_trip(config, &deliveries);
    assert_eq!(recording.key(0), None);
    assert!((1..16).any(|round| recording.key(round) == Some(0xcc)));
    assert_eq!(recording.rounds, 16);
}

#[test]
fn repeated_keys_pause_death_and_restart_reexecute() {
    let config = configuration(&[(5, 15, Tile::Player), (25, 15, Tile::Gold)]);
    let keys = [
        Some(0xcc),
        Some(0xcc),
        Some(0x9b),
        None,
        Some(0x9b),
        Some(0x81),
        None,
        Some(0xa0),
        None,
    ];
    let recording = round_trip(config, &keys);
    assert_eq!(recording.key(0), Some(0xcc));
    assert_eq!(recording.key(1), Some(0xcc));
}

#[test]
fn malformed_recordings_and_unsupported_observations_are_rejected() {
    let recording = Recording::new(configuration(&[(5, 15, Tile::Player)]));
    let text = recording.to_json().unwrap();
    assert!(Recording::from_json(&text.replace("\"version\": 2", "\"version\": 3")).is_err());
    assert!(
        Recording::from_json(&text.replace("\"inputs\": {}", "\"inputs\": {\"0\": 204}")).is_err()
    );
    assert!(Recording::from_json(&text.replace("\"rounds\": 0,", "")).is_err());
    let mut recording = recording;
    assert!(
        recording
            .push(&[lode_trace::Input {
                observation: 1,
                key: 0xcc
            }])
            .is_err()
    );
    assert_eq!(recording.rounds, 0);
}

// Idealized inputs reproduce every gameplay snapshot. Timing and host sound
// preferences remain properties of the diagnostic trace rather than this file.
fn capture_round_trip(
    configuration: Configuration,
    deliveries: &[Option<u8>],
    prepare: impl Fn(usize, &mut Run),
) -> Recording {
    let mut live = Run::new(configuration.clone(), false).unwrap();
    let mut recording = Recording::new(configuration);
    let mut snapshots = vec![];
    for (round, &delivery) in deliveries.iter().enumerate() {
        prepare(round, &mut live);
        live.step(delivery).unwrap();
        recording.capture(&live).unwrap();
        snapshots.push(live.game.snapshot());
    }
    let recording = Recording::from_json(&recording.to_json().unwrap()).unwrap();
    let mut replay = Run::new(recording.configuration.clone(), false).unwrap();
    for (round, expected) in snapshots.into_iter().enumerate() {
        prepare(round, &mut replay);
        replay.step(recording.key(round)).unwrap();
        assert_eq!(replay.game.snapshot(), expected, "round {round}");
    }
    recording
}

#[test]
fn capture_omits_host_controls_repeats_and_rejected_actions() {
    let config = configuration(&[(5, 15, Tile::Player), (25, 15, Tile::Gold)]);
    let recording = capture_round_trip(
        config,
        &[
            Some(0xc9), // No ladder: Up is ineffective.
            Some(0xd5), // No brick below: digging is ineffective.
            Some(0x93), // Sound is a host preference.
            Some(0x88), // Speed does not change per-round gameplay.
            Some(0xcc),
            Some(0xcc), // The persistent direction is already Right.
            Some(0x8d), // Return leaves the command intact.
            Some(0xa0),
            Some(0xa0),
            Some(0x95),
        ],
        |_, _| {},
    );
    assert_eq!(recording.key(4), Some(0xcc));
    assert_eq!(recording.key(7), Some(0xa0));
    assert!(
        (0..recording.rounds)
            .filter_map(|round| recording.key(round))
            .eq([0xcc, 0xa0])
    );
}

#[test]
fn rejected_direction_or_dig_can_supply_a_needed_stop() {
    for rejected in [0xc9, 0xd5] {
        let config = configuration(&[(5, 15, Tile::Player), (25, 15, Tile::Gold)]);
        let recording =
            capture_round_trip(config, &[Some(0xcc), Some(rejected), None, None], |_, _| {});
        assert_eq!(recording.key(0), Some(0xcc));
        assert_eq!(recording.key(1), Some(0xa0));
        assert_eq!(recording.key(2), None);
    }
}

#[test]
fn initially_rejected_direction_is_recorded_when_it_becomes_effective() {
    for control in [None, Some(0x81)] {
        let config = configuration(&[(5, 15, Tile::Player), (25, 15, Tile::Gold)]);
        let recording =
            capture_round_trip(config, &[Some(0xcc), None, None, control], |round, run| {
                run.game.board.cells[15 * 28 + 6] =
                    if round < 3 { Tile::Stone } else { Tile::Empty };
            });
        if control.is_some() {
            assert_eq!(recording.key(0), Some(0xcc));
            assert_eq!(recording.key(3), control);
        } else {
            assert_eq!(recording.key(0), None);
            assert_eq!(recording.key(3), Some(0xcc));
        }
    }
}

#[test]
fn pause_resume_and_ignored_paused_keys_reproduce_frozen_gameplay() {
    let config = configuration(&[(5, 15, Tile::Player), (25, 15, Tile::Gold)]);
    let recording = capture_round_trip(
        config,
        &[
            Some(0xcc),
            Some(0x9b),
            Some(0xca),
            Some(0x93),
            Some(0x9b),
            None,
        ],
        |_, _| {},
    );
    assert_eq!(recording.key(0), Some(0xcc));
    assert_eq!(recording.key(1), Some(0x9b));
    assert_eq!(recording.key(2), None);
    assert_eq!(recording.key(3), None);
    assert_eq!(recording.key(4), Some(0x9b));
    assert_eq!(recording.key(5), None);
}

#[test]
fn rejected_navigation_has_no_recorded_interval() {
    let config = configuration(&[(5, 15, Tile::Player), (25, 15, Tile::Gold)]);
    let mut run = Run::new(config.clone(), false).unwrap();
    let mut recording = Recording::new(config);
    for key in [0x82, 0x83] {
        run.step(Some(key)).unwrap();
        recording.capture(&run).unwrap();
    }
    assert_eq!(recording.rounds, 0);
    run.step(Some(0xcc)).unwrap();
    recording.capture(&run).unwrap();
    assert_eq!(recording.rounds, 1);
    assert_eq!(recording.key(0), Some(0xcc));
}

#[test]
fn smart_capture_leaves_diagnostic_observations_exact() {
    let config = configuration(&[(5, 15, Tile::Player), (25, 15, Tile::Gold)]);
    let mut run = Run::new(config.clone(), false).unwrap();
    let mut recording = Recording::new(config);
    let keys = [0xcc, 0xcc, 0x93, 0xc9];
    for key in keys {
        run.step(Some(key)).unwrap();
        recording.capture(&run).unwrap();
    }
    let trace = run.trace().unwrap();
    for (round, key) in trace.rounds.iter().zip(keys) {
        assert_eq!(round.inputs[0].key, key);
    }
    assert_eq!(recording.key(0), Some(0xcc));
    assert_eq!(recording.key(1), None);
    assert_eq!(recording.key(2), None);
    assert_eq!(recording.key(3), Some(0xa0));
}

#[test]
fn original_boards_reproduce_gameplay_through_deaths_and_reloads() {
    for level in [1, 10, 50, 150] {
        let mut config = Configuration::original(level).unwrap();
        config.wait_on_level_start = false;
        let mut live = Run::new(config.clone(), false).unwrap();
        let mut recording = Recording::new(config);
        let mut snapshots = vec![];
        let keys = [0xcc, 0xcc, 0xd5, 0xc9, 0x93, 0xca, 0xcf, 0xcb, 0xa0, 0x95];
        for round in 0..400 {
            if matches!(live.game.session.phase, Phase::Died | Phase::LevelCleared) {
                live.step(None).unwrap();
            }
            let key = (round % 3 == 0).then_some(keys[(round / 3) % keys.len()]);
            live.step(key).unwrap();
            recording.capture(&live).unwrap();
            snapshots.push(live.game.snapshot());
        }
        let mut replay = Run::new(recording.configuration.clone(), false).unwrap();
        for (round, expected) in snapshots.into_iter().enumerate() {
            if matches!(replay.game.session.phase, Phase::Died | Phase::LevelCleared) {
                replay.step(None).unwrap();
            }
            replay.step(recording.key(round)).unwrap();
            assert_eq!(
                replay.game.snapshot(),
                expected,
                "level {level}, round {round}"
            );
        }
    }
}
