use lode_trace::{Phase, Tile};
use loderunner::collision::{Layer, Scene};
use loderunner::devices::keyboard::LevelNavigation;
use loderunner::session::{Configuration, Run, replay};
use loderunner::storage::LevelRecord;
use loderunner::timing::NTSC_CPU_HZ;
use loderunner::transition::{BONUS_DURATION_US, IRIS_DURATION_CYCLES, IRIS_FRAMES, VisualEffect};

fn level(col: usize, row: usize) -> LevelRecord {
    let mut bytes = [0; 256];
    for (col, row, tile) in [(col, row, Tile::Player), (col, row + 1, Tile::Brick)] {
        if row < 16 {
            let index = row * 28 + col;
            bytes[index / 2] |= (tile as u8) << ((index % 2) * 4);
        }
    }
    LevelRecord(bytes)
}

#[test]
fn ready_waits_are_untraced_and_the_first_key_moves_and_is_recorded() {
    let mut run = Run::new(
        Configuration {
            levels: vec![level(5, 15)],
            start: 0,
            wait_on_level_start: true,
        },
        false,
    )
    .unwrap();
    let initial = run.trace().unwrap();
    run.present_initial(&mut |_| Ok(())).unwrap();
    let picture = run.game.scene.row_bits(Layer::Foreground, 170);
    for _ in 0..100 {
        run.step(None).unwrap();
    }
    assert!(matches!(
        run.game.visual_effect,
        VisualEffect::HidePlayer { .. }
    ));
    assert_eq!(run.game.scene.row_bits(Layer::Foreground, 170), picture);
    assert_eq!(run.trace().unwrap(), initial);
    run.step(Some(0xcc)).unwrap();
    assert_eq!(run.game.session.phase, Phase::Playing);
    assert_eq!(run.game.player.position.x(), 3);
    assert_eq!(run.last_inputs()[0].key, 0xcc);
    assert_eq!(run.trace().unwrap().rounds.len(), 1);
    run.step(Some(0xca)).unwrap();
    assert_eq!(replay(&run.trace().unwrap()).unwrap(), run.trace().unwrap());
}

#[test]
fn starting_commands_match_gameplay_with_an_automatically_dismissed_ready_screen() {
    let mut board = level(5, 14);
    let index = 15 * 28 + 6;
    board.0[index / 2] |= (Tile::Brick as u8) << ((index % 2) * 4);
    let config = Configuration {
        levels: vec![board],
        start: 0,
        wait_on_level_start: true,
    };
    for key in [0xcc, 0xcf, 0x9b] {
        let mut live = Run::new(config.clone(), false).unwrap();
        let mut playback = Run::new(config.clone(), false).unwrap();
        for _ in 0..10 {
            live.step(None).unwrap();
        }
        playback.start_ready_level();
        live.step(Some(key)).unwrap();
        playback.step(Some(key)).unwrap();
        assert_eq!(live.trace().unwrap(), playback.trace().unwrap());
        assert_eq!(live.last_inputs()[0].key, key);
        if key == 0xcf {
            assert!(live.game.player.digging.is_some());
        }
    }
}

#[test]
fn airborne_start_retains_the_first_key_until_gameplay_can_observe_it() {
    let mut board = level(5, 0);
    let index = 28 + 5;
    board.0[index / 2] &= !(15 << ((index % 2) * 4));
    let mut run = Run::new(
        Configuration {
            levels: vec![board],
            start: 0,
            wait_on_level_start: true,
        },
        false,
    )
    .unwrap();
    run.step(Some(0xcc)).unwrap();
    assert!(run.last_inputs().is_empty());
    for _ in 0..100 {
        run.step(None).unwrap();
        if !run.last_inputs().is_empty() {
            break;
        }
    }
    assert_eq!(run.last_inputs()[0].key, 0xcc);
    assert_eq!(replay(&run.trace().unwrap()).unwrap(), run.trace().unwrap());
}

#[test]
fn completion_animates_score_before_switching_boards_without_gameplay_time() {
    let mut run = Run::new(
        Configuration {
            levels: vec![level(5, 0), level(8, 15)],
            start: 0,
            wait_on_level_start: false,
        },
        false,
    )
    .unwrap();
    run.game.session.lives_used = 8;
    run.step(Some(0xa0)).unwrap();
    assert_eq!(run.game.session.phase, Phase::LevelCleared);
    let gameplay = run.game.clock.cycles;
    let output = run.game.clock.output_cycles();
    let mut scores = Vec::new();
    let mut closed = false;
    run.step_with_checkpoint(None, &mut |game| {
        assert_eq!(game.clock.cycles, gameplay);
        if game.visual_effect == VisualEffect::None && game.transitioning {
            assert_eq!(game.player.position.cell.col, 5);
            scores.push(game.score.decimal());
        }
        if game.visual_effect == (VisualEffect::Iris { frame: 0 }) {
            closed = true;
            assert_eq!(game.player.position.cell.col, 5);
        }
        if game.player.position.cell.col == 8 {
            assert!(closed);
        }
        assert!(
            game.speaker
                .edges
                .iter()
                .all(|edge| edge.cycle <= game.clock.output_cycles())
        );
        Ok(())
    })
    .unwrap();
    assert_eq!(scores, (1..=15).map(|step| step * 100).collect::<Vec<_>>());
    assert_eq!(run.game.session.lives_used, 1);
    assert_eq!(
        run.game.clock.output_cycles() - output,
        BONUS_DURATION_US * NTSC_CPU_HZ / 1_000_000 + 2 * IRIS_DURATION_CYCLES
    );
    assert_eq!(run.trace().unwrap().final_delta, 0);
}

#[test]
fn circular_iris_preserves_status_and_expands_monotonically() {
    let mut source = Scene::default();
    for row in 0..16 {
        for col in 0..28 {
            source.tile(Layer::Foreground, 2, col, row);
        }
    }
    source.status_border();
    let status = source.row_bits(Layer::Foreground, 180);
    let original = source.row_bits(Layer::Foreground, 88);
    let mut previous = 0;
    for frame in 0..=IRIS_FRAMES {
        let mut picture = source.clone();
        VisualEffect::Iris { frame }.apply(&mut picture);
        let count: u32 = (0..176)
            .map(|row| {
                picture
                    .row_bits(Layer::Foreground, row)
                    .iter()
                    .map(|bits| bits.count_ones())
                    .sum::<u32>()
            })
            .sum();
        assert!(count >= previous);
        if frame == 0 {
            assert_eq!(count, 0);
        }
        if frame == IRIS_FRAMES {
            assert_eq!(picture.row_bits(Layer::Foreground, 88), original);
        }
        assert_eq!(picture.row_bits(Layer::Foreground, 180), status);
        previous = count;
    }
    assert_eq!(source.row_bits(Layer::Foreground, 88), original);
}

#[test]
fn navigation_bypasses_terminal_sequences_and_stops_at_list_endpoints() {
    let config = Configuration {
        levels: vec![level(5, 15), level(8, 15)],
        start: 0,
        wait_on_level_start: true,
    };
    let mut run = Run::new(config, false).unwrap();
    let initial = run.game.snapshot();
    run.step(Some(LevelNavigation::Previous as u8)).unwrap();
    assert_eq!(run.game.snapshot(), initial);
    assert_eq!(run.game.clock.output_cycles(), 0);

    let mut frames = Vec::new();
    run.step_with_checkpoint(Some(LevelNavigation::Next as u8), &mut |game| {
        assert!(game.player.alive);
        assert_eq!(game.score.decimal(), 0);
        assert_eq!(game.clock.cycles, 0);
        assert!(game.speaker.edges.is_empty());
        if let VisualEffect::Iris { frame } = game.visual_effect {
            frames.push((game.player.position.cell.col, frame));
        } else {
            panic!("navigation presented a tune checkpoint");
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(frames.len(), 2 * IRIS_FRAMES as usize);
    assert_eq!(frames[IRIS_FRAMES as usize - 1], (5, 0));
    assert_eq!(frames[IRIS_FRAMES as usize], (8, 1));
    assert_eq!(run.game.clock.output_cycles(), NTSC_CPU_HZ);
    assert_eq!(run.game.session.phase, Phase::Waiting);
    assert_eq!(run.game.session.level, 2);
    let last = run.game.snapshot();
    let output = run.game.clock.output_cycles();
    run.step(Some(LevelNavigation::Next as u8)).unwrap();
    assert_eq!(run.game.snapshot(), last);
    assert_eq!(run.game.clock.output_cycles(), output);
    run.step(Some(LevelNavigation::Previous as u8)).unwrap();
    assert_eq!(run.game.session.level, 1);
    assert_eq!(run.game.difficulty, 0);
    run.step(Some(0xcc)).unwrap();
    assert_eq!(run.game.player.position.x(), 3);
    assert_eq!(run.last_inputs()[0].key, 0xcc);
    assert_eq!(replay(&run.trace().unwrap()).unwrap(), run.trace().unwrap());
}

#[test]
fn navigation_interrupts_falling_and_pause_without_advancing_gameplay() {
    for pause in [false, true] {
        let mut run = Run::new(
            Configuration {
                levels: vec![level(5, if pause { 15 } else { 0 }), level(8, 15)],
                start: 0,
                wait_on_level_start: false,
            },
            false,
        )
        .unwrap();
        if pause {
            run.step(Some(0x9b)).unwrap();
        } else {
            run.game.board.cells[28 + 5] = Tile::Empty;
            run.game.board.terrain[28 + 5] = Tile::Empty;
            run.step(None).unwrap();
            assert_eq!(run.game.player.motion, lode_trace::Motion::Falling);
        }
        let cycles = run.game.clock.cycles;
        let edges = run.game.speaker.edges.len();
        run.step(Some(LevelNavigation::Next as u8)).unwrap();
        assert_eq!(run.game.clock.cycles, cycles);
        assert_eq!(run.game.speaker.edges.len(), edges);
        assert_eq!(run.game.session.level, 2);
        assert_eq!(run.game.session.phase, Phase::Playing);
    }
}

#[test]
fn direct_navigation_preserves_score_and_difficulty_and_resets_lives_used() {
    let mut run = Run::new(
        Configuration {
            levels: vec![level(5, 15), level(8, 15)],
            start: 0,
            wait_on_level_start: true,
        },
        false,
    )
    .unwrap();
    run.game.score.add(250);
    run.game.session.lives_used = 8;
    run.game.difficulty = 3;
    run.step(Some(LevelNavigation::Next as u8)).unwrap();
    assert_eq!(run.game.score.decimal(), 250);
    assert_eq!(run.game.session.lives_used, 1);
    assert_eq!(run.game.difficulty, 3);
}
