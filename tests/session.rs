use lode_trace::{Change, Facing, Life, Motion, Phase, Tile, Trace};
use loderunner::session::{Configuration, Run, capture, replay};
use loderunner::storage::{LevelRecord, original_level};

fn record(tiles: &[(usize, usize, Tile)]) -> LevelRecord {
    let mut record = [0; 256];
    for &(col, row, tile) in tiles {
        let index = row * 28 + col;
        record[index / 2] |= (tile as u8) << ((index % 2) * 4);
    }
    LevelRecord(record)
}

fn configuration(levels: Vec<LevelRecord>) -> Configuration {
    Configuration {
        levels,
        start: 0,
        wait_on_level_start: false,
    }
}

#[test]
fn sacrifice_finishes_movement_and_reload_retains_direction() {
    let config = configuration(vec![record(&[(5, 15, Tile::Player)])]);
    let dead = capture(config.clone(), &[Some(0xca), Some(0x81)]).unwrap();
    assert_eq!(dead.final_state.actors[0].x_adjust, 0);
    assert_eq!(dead.final_state.actors[0].motion, Motion::Left);
    assert_eq!(dead.final_state.actors[0].life, Life::Dead);
    let restarted = capture(config, &[Some(0xca), Some(0x81), None]).unwrap();
    assert_eq!(restarted.final_state.actors[0].x_adjust, 2);
    assert_eq!(restarted.final_state.actors[0].facing, Facing::Left);
    assert_eq!(restarted.final_state.session.attempt, 1);
    assert_eq!(replay(&restarted).unwrap(), restarted);
}

#[test]
fn airborne_death_preserves_whistle_across_reload_and_new_game() {
    let config = configuration(vec![record(&[
        (5, 0, Tile::Player),
        (6, 1, Tile::Guard),
        (6, 2, Tile::Brick),
    ])]);
    let mut run = Run::new(config, false).unwrap();
    let mut deaths = 0;
    for _ in 0..100 {
        if run.game.session.phase == Phase::Died {
            let pitch = run.game.falling_pitch;
            assert!(run.game.falling_sound);
            run.step(None).unwrap();
            // The death tune finishes with an empty sound call, which advances
            // the retained falling whistle once before the board is reloaded.
            assert_eq!(run.game.falling_pitch, pitch.wrapping_add(1));
            assert!(run.game.falling_sound);
            deaths += 1;
        } else if run.game.session.phase == Phase::GameOver {
            let pitch = run.game.falling_pitch;
            run.step(Some(0xa0)).unwrap();
            assert_eq!(run.game.falling_pitch, pitch);
            assert!(run.game.falling_sound);
            assert_eq!(run.game.session.attempt, 0);
            assert_eq!(deaths, 5);
            assert_eq!(replay(&run.trace().unwrap()).unwrap(), run.trace().unwrap());
            return;
        } else {
            run.step(None).unwrap();
        }
    }
    panic!("airborne collisions did not reach game over");
}

#[test]
fn full_guarded_rounds_round_trip_on_every_original_level() {
    for level in 1..=150 {
        let configuration = configuration(vec![LevelRecord(original_level(level).unwrap())]);
        let mut inputs = vec![None; 90];
        for (round, key) in [
            (0, 0xcc),
            (11, 0xa0),
            (20, 0xca),
            (31, 0xd5),
            (45, 0xcf),
            (60, 0xc9),
            (75, 0xcb),
        ] {
            inputs[round] = Some(key);
        }
        let original = capture(configuration, &inputs)
            .unwrap_or_else(|error| panic!("level {level}: {error}"));
        let decoded = Trace::from_json(&original.to_json().unwrap()).unwrap();
        assert_eq!(
            replay(&decoded).unwrap_or_else(|error| panic!("level {level}: {error}")),
            decoded
        );
    }
}

#[test]
fn death_restart_and_game_over_are_separate_observable_intervals() {
    let config = configuration(vec![record(&[
        (5, 15, Tile::Player),
        (15, 15, Tile::Guard),
    ])]);
    let mut inputs = Vec::new();
    for _ in 0..5 {
        inputs.extend([Some(0x81), None]);
    }
    let trace = capture(config, &inputs).unwrap();
    assert_eq!(trace.final_state.session.phase, Phase::GameOver);
    assert_eq!(trace.final_state.session.lives, 0);
    assert_eq!(trace.final_state.session.attempt, 4);
    assert_eq!(replay(&trace).unwrap(), trace);
}

#[test]
fn level_advancement_awards_score_and_loads_next_board() {
    let config = configuration(vec![
        record(&[(5, 0, Tile::Player), (5, 1, Tile::Brick)]),
        record(&[(8, 15, Tile::Player), (20, 15, Tile::Guard)]),
    ]);
    let trace = capture(config, &[Some(0xa0), None, Some(0xcc)]).unwrap();
    assert_eq!(trace.final_state.session.level, 2);
    assert_eq!(trace.final_state.session.score, 1500);
    assert_eq!(trace.final_state.session.lives, 6);
    assert_eq!(trace.final_state.actors[0].cell.col, 8);
    assert_eq!(replay(&trace).unwrap(), trace);
}

#[test]
fn pause_and_sound_speed_controls_round_trip() {
    let config = configuration(vec![record(&[
        (5, 15, Tile::Player),
        (20, 15, Tile::Guard),
    ])]);
    let trace = capture(
        config,
        &[
            Some(0x9b),
            None,
            Some(0xa0),
            Some(0x9b),
            Some(0x93),
            Some(0x95),
            Some(0x88),
            Some(0xcc),
        ],
    )
    .unwrap();
    assert_eq!(replay(&trace).unwrap(), trace);
    assert!(trace.rounds[0].changes.iter().any(
        |change| matches!(change, Change::Session { after, .. } if after.phase == Phase::Paused)
    ));
}

#[test]
fn valid_but_wrong_gameplay_and_timing_traces_fail_reexecution() {
    let config = configuration(vec![record(&[(5, 15, Tile::Player)])]);
    let mut trace = capture(config, &[Some(0xcc)]).unwrap();
    for change in &mut trace.rounds[0].changes {
        if let Change::Actor {
            after: Some(actor), ..
        } = change
        {
            actor.motion = Motion::Left;
        }
    }
    trace.final_state.actors[0].motion = Motion::Left;
    trace.validate().unwrap();
    assert!(replay(&trace).unwrap_err().contains("round 0"));
    trace.rounds[0].start_delta += 1;
    assert!(replay(&trace).is_err());
}

#[test]
fn scored_game_over_restarts_without_initials() {
    let config = configuration(vec![record(&[(5, 15, Tile::Player), (6, 15, Tile::Gold)])]);
    let mut inputs = vec![Some(0xcc), None, None, None, None, Some(0xa0)];
    for _ in 0..5 {
        inputs.extend([Some(0x81), None]);
    }
    let trace = capture(config.clone(), &inputs).unwrap();
    assert_eq!(trace.final_state.session.phase, Phase::GameOver);
    assert_eq!(trace.final_state.session.score, 250);
    assert_eq!(replay(&trace).unwrap(), trace);
    inputs.push(Some(0xc1));
    let restarted = capture(config, &inputs).unwrap();
    assert_eq!(restarted.final_state.session.phase, Phase::Playing);
    assert_eq!(restarted.final_state.session.score, 0);
    assert_eq!(restarted.final_state.session.lives, 5);
}

#[test]
fn return_keeps_playing_without_opening_a_menu() {
    let config = configuration(vec![record(&[
        (5, 15, Tile::Player),
        (20, 15, Tile::Guard),
    ])]);
    let keys = [Some(0xcc), Some(0x8d), None, Some(0xa0)];
    let trace = capture(config.clone(), &keys).unwrap();
    let uninterrupted = capture(config, &[Some(0xcc), None, None, Some(0xa0)]).unwrap();
    assert_eq!(trace.final_state, uninterrupted.final_state);
    assert!(trace.rounds.iter().all(|round| round.changes.iter().all(|change| {
        !matches!(change, lode_trace::Change::Session { after, .. } if after.phase != Phase::Playing)
    })));
    assert_eq!(replay(&trace).unwrap(), trace);
    assert_eq!(trace.final_state.session.phase, Phase::Playing);
}
