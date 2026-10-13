use lode_original::rounds::{capture_guard_free, replay_guard_free};
use lode_original::state::Game;
use lode_trace::{Phase, Tile, Trace};

fn record(tiles: &[(usize, usize, Tile)]) -> [u8; 256] {
    let mut record = [0; 256];
    for &(col, row, tile) in tiles {
        let index = row * 28 + col;
        let shift = (index % 2) * 4;
        record[index / 2] |= (tile as u8) << shift;
    }
    record
}

// This round trip executes movement again from decoded level data. Expected
// actor changes never enter the execution driver; only recorded inputs do.
#[test]
fn serialized_player_rounds_reexecute_identically() {
    let board = record(&[(5, 15, Tile::Player), (6, 15, Tile::Gold)]);
    let mut inputs = vec![None; 16];
    inputs[0] = Some(0xcc);
    inputs[5] = Some(0xa0);
    inputs[8] = Some(0xca);
    inputs[13] = Some(0xa0);
    let captured = capture_guard_free(&board, 1, &inputs).unwrap();
    let trace = Trace::from_json(&captured.to_json().unwrap()).unwrap();
    assert_eq!(replay_guard_free(&trace, &board, 1).unwrap(), trace);
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
    let board = record(&[(5, 15, Tile::Player)]);
    let trace = capture_guard_free(&board, 1, &[Some(0xcc), None]).unwrap();
    let mut changed = trace.clone();
    changed.rounds[1].start_delta += 1;
    assert!(
        replay_guard_free(&changed, &board, 1)
            .unwrap_err()
            .contains("round 1")
    );
    assert!(
        replay_guard_free(&trace, &board, 2)
            .unwrap_err()
            .contains("configuration")
    );
    let guarded = record(&[(5, 15, Tile::Player), (8, 15, Tile::Guard)]);
    assert!(
        capture_guard_free(&guarded, 1, &[None])
            .unwrap_err()
            .contains("guard-free")
    );
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
    let trace = capture_guard_free(&board, 1, &inputs).unwrap();
    assert_eq!(replay_guard_free(&trace, &board, 1).unwrap(), trace);
    assert!(trace.rounds.iter().any(|round| round.changes.iter().any(
        |change| matches!(change, lode_trace::Change::Holes { after, .. } if !after.is_empty())
    )));
    assert!(trace.final_state.holes.is_empty());
    assert_eq!(trace.final_state.active[15 * 28 + 6], Tile::Brick);
}

#[test]
fn closing_hole_kills_player_and_journals_environment_contact() {
    let board = record(&[(5, 15, Tile::Player)]);
    let mut game = Game::load_level(&board, 1, false).unwrap();
    game.holes.cols[0] = 5;
    game.holes.rows[0] = 15;
    game.holes.timers[0] = 1;
    let mut journal = vec![];
    game.handle_hole_timers(&mut journal).unwrap();
    assert!(!game.alive);
    assert_eq!(journal.len(), 1);
    assert_eq!(game.active[15 * 28 + 5], Tile::Brick);
}

#[test]
fn terminal_round_closes_without_resetting_actor_state() {
    let board = record(&[(5, 0, Tile::Player), (5, 1, Tile::Brick)]);
    let trace = capture_guard_free(&board, 1, &[Some(0xa0)]).unwrap();
    assert_eq!(trace.final_state.session.phase, Phase::LevelCleared);
    assert_eq!(replay_guard_free(&trace, &board, 1).unwrap(), trace);
    assert!(capture_guard_free(&board, 1, &[None, None]).is_err());
}
