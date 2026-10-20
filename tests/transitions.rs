use lode_trace::{Phase, Tile};
use loderunner::collision::{Layer, Scene};
use loderunner::session::{Configuration, Run, replay};
use loderunner::storage::LevelRecord;
use loderunner::timing::NTSC_CPU_HZ;
use loderunner::transition::{BONUS_DURATION_US, IRIS_FRAMES, VisualEffect};

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
fn startup_and_ready_waits_leave_no_timing_or_input_in_trace() {
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
    assert_eq!(run.trace().unwrap(), initial);
    assert!(run.last_inputs().is_empty());
    run.step(Some(0xca)).unwrap();
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
        BONUS_DURATION_US * NTSC_CPU_HZ / 1_000_000 + 2 * NTSC_CPU_HZ
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
