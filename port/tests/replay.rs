use lode_port::board::OpenHole;
use lode_port::session::{Configuration, capture, replay};
use lode_port::state::Game;
use lode_port::storage::LevelRecord;
use lode_trace::{Cell, Phase, Tile, Trace};
use std::num::NonZeroU8;

fn record(tiles: &[(usize, usize, Tile)]) -> [u8; 256] {
    let mut record = [0; 256];
    for &(col, row, tile) in tiles {
        let index = row * 28 + col;
        record[index / 2] |= (tile as u8) << ((index % 2) * 4);
    }
    record
}
fn capture_board(board: [u8; 256], inputs: &[Option<u8>]) -> Result<Trace, String> {
    capture(
        Configuration {
            levels: vec![LevelRecord(board)],
            start: 0,
            high_scores: vec![],
            wait_on_level_start: false,
        },
        inputs,
    )
}

#[test]
fn serialized_player_rounds_reexecute_identically() {
    let board = record(&[(5, 15, Tile::Player), (6, 15, Tile::Gold)]);
    let mut inputs = vec![None; 16];
    inputs[0] = Some(0xcc);
    inputs[5] = Some(0xa0);
    inputs[8] = Some(0xca);
    inputs[13] = Some(0xa0);
    let captured = capture_board(board, &inputs).unwrap();
    let trace = Trace::from_json(&captured.to_json().unwrap()).unwrap();
    assert_eq!(replay(&trace).unwrap(), trace);
    assert_eq!(trace.final_state.session.score, 250);
    assert_eq!(
        trace
            .rounds
            .iter()
            .map(|round| round.inputs.len())
            .sum::<usize>(),
        4
    );
    assert!(trace.rounds[1].start_delta > 0);
}

#[test]
fn timing_and_configuration_mismatches_are_rejected() {
    let trace = capture_board(record(&[(5, 15, Tile::Player)]), &[Some(0xcc), None]).unwrap();
    let mut changed = trace.clone();
    changed.rounds[1].start_delta += 1;
    assert!(replay(&changed).unwrap_err().contains("round 1"));
    let mut config: Configuration = serde_json::from_str(&trace.header.configuration).unwrap();
    config.start = 1;
    changed = trace;
    changed.header.configuration = serde_json::to_string(&config).unwrap();
    assert!(replay(&changed).is_err());
}

#[test]
fn round_trip_includes_hole_opening_and_refill() {
    let board = record(&[
        (5, 14, Tile::Player),
        (5, 15, Tile::Brick),
        (6, 15, Tile::Brick),
    ]);
    let mut inputs = vec![None; 193];
    inputs[0] = Some(0xcf);
    let trace = capture_board(board, &inputs).unwrap();
    assert_eq!(replay(&trace).unwrap(), trace);
    assert!(trace.rounds.iter().any(|round| {
        round.changes.iter().any(
            |change| matches!(change,lode_trace::Change::Holes {after,..} if !after.is_empty()),
        )
    }));
    assert!(trace.final_state.holes.is_empty());
    assert_eq!(trace.final_state.active[15 * 28 + 6], Tile::Brick);
}

#[test]
fn closing_hole_kills_player_and_journals_environment_contact() {
    let level = LevelRecord(record(&[(5, 15, Tile::Player)]))
        .decode()
        .unwrap();
    let mut game = Game::load_level(&level, 1, false);
    game.holes.entries[0] = Some(OpenHole {
        cell: Cell::new(5, 15).unwrap(),
        remaining: NonZeroU8::new(1).unwrap(),
        generation: 1,
    });
    let mut journal = vec![];
    game.update_holes(&mut journal).unwrap();
    assert!(!game.player.alive);
    assert_eq!(journal.len(), 1);
    assert_eq!(game.board.cells[15 * 28 + 5], Tile::Brick);
}

#[test]
fn terminal_round_closes_without_resetting_actor_state() {
    let trace = capture_board(
        record(&[(5, 0, Tile::Player), (5, 1, Tile::Brick)]),
        &[Some(0xa0)],
    )
    .unwrap();
    assert_eq!(trace.final_state.session.phase, Phase::LevelCleared);
    assert_eq!(replay(&trace).unwrap(), trace);
}
