use lode_trace::Tile;
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
        original.step(delivery).unwrap();
        recording.push(original.last_inputs()).unwrap();
    }
    let recording = Recording::from_json(&recording.to_json().unwrap()).unwrap();
    let mut playback = Run::new(recording.configuration.clone(), false).unwrap();
    for round in 0..recording.rounds {
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
    assert!(Recording::from_json(&text.replace("\"version\": 1", "\"version\": 2")).is_err());
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
