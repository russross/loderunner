use lode_trace::{Phase, Tile};
use loderunner::recording::OverridePlayback;
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

#[test]
fn optional_overrides_round_trip_and_validate_at_the_boundary() {
    let recording = Recording::new(configuration(&[(5, 15, Tile::Player)]));
    let mut value = serde_json::to_value(recording).unwrap();
    assert!(value.get("overrides").is_none());
    value["overrides"] = serde_json::json!({
        "gold_delays": [37, null, 1, 255],
        "resurrection_columns": [27, null, 0]
    });
    let parsed = Recording::from_json(&value.to_string()).unwrap();
    assert_eq!(serde_json::to_value(parsed).unwrap(), value);
    value["overrides"]["gold_delays"][0] = serde_json::json!(0);
    assert!(Recording::from_json(&value.to_string()).is_err());
    value["overrides"]["gold_delays"][0] = serde_json::json!(1);
    value["overrides"]["resurrection_columns"][0] = serde_json::json!(28);
    assert!(Recording::from_json(&value.to_string()).is_err());
}

// These consumed-input fixtures come from Total Recall's demoData1 at revision
// cceca5d5a15a21f724836654a46bf2501968e142, translated from its tick deliveries.
fn converted_demo(text: &str) -> Recording {
    let mut value: serde_json::Value = serde_json::from_str(text).unwrap();
    let level = value
        .as_object_mut()
        .unwrap()
        .remove("level")
        .unwrap()
        .as_u64()
        .unwrap();
    value["version"] = serde_json::json!(2);
    value["configuration"] =
        serde_json::to_value(Configuration::original(level as u8).unwrap()).unwrap();
    Recording::from_json(&value.to_string()).unwrap()
}

fn play_converted(recording: &Recording, delays: bool, columns: bool) -> Run {
    let mut run = Run::new(recording.configuration.clone(), false).unwrap();
    let mut overrides = recording.overrides.clone();
    if !delays {
        overrides.gold_delays.clear();
    }
    if !columns {
        overrides.resurrection_columns.clear();
    }
    run.game.recording_overrides = OverridePlayback::new(overrides);
    run.start_ready_level();
    for round in 0..recording.rounds {
        run.step(recording.key(round)).unwrap();
        if matches!(run.game.session.phase, Phase::Died | Phase::LevelCleared) {
            break;
        }
    }
    run
}

#[test]
fn converted_level_11_needs_both_delay_and_column_overrides_to_win() {
    let recording = converted_demo(include_str!("fixtures/total-recall-11.json"));
    let win = play_converted(&recording, true, true);
    assert_eq!(win.game.session.phase, Phase::LevelCleared);
    for (delays, columns) in [(false, false), (true, false), (false, true)] {
        let run = play_converted(&recording, delays, columns);
        assert_ne!(run.game.session.phase, Phase::LevelCleared);
    }
}

#[test]
fn converted_level_64_columns_change_survival_but_do_not_resolve_other_differences() {
    let recording = converted_demo(include_str!("fixtures/total-recall-64.json"));
    let with_columns = play_converted(&recording, true, true);
    let without_columns = play_converted(&recording, true, false);
    assert_eq!(with_columns.game.session.phase, Phase::Died);
    assert_eq!(without_columns.game.session.phase, Phase::Died);
    assert!(
        with_columns.trace().unwrap().rounds.len() > without_columns.trace().unwrap().rounds.len()
    );
}
