use lode_trace::{Phase, Tile};
use loderunner::event_timing::{TimingEvent, Work};
use loderunner::session::{Configuration, Run, replay};
use loderunner::storage::LevelRecord;

fn run(tiles: &[(usize, usize, Tile)]) -> Run {
    let mut bytes = [0; 256];
    for &(col, row, tile) in tiles {
        let index = row * 28 + col;
        bytes[index / 2] |= (tile as u8) << ((index % 2) * 4);
    }
    Run::new(
        Configuration {
            levels: vec![LevelRecord(bytes)],
            start: 0,
            wait_on_level_start: false,
        },
        false,
    )
    .unwrap()
}

// Complete turns publish model time before their single host checkpoint. Draw
// and sound records retain inputs, with no measured cycle totals in the journal.
#[test]
fn complete_turn_commits_model_time_and_default_trace_reexecutes() {
    let mut run = run(&[(5, 15, Tile::Player), (25, 15, Tile::Gold)]);
    let mut checkpoints = Vec::new();
    run.step_with_checkpoint(Some(0xcc), &mut |game| {
        checkpoints.push(game.clock.cycles);
        assert!(
            game.speaker
                .edges
                .iter()
                .all(|edge| edge.cycle <= game.clock.cycles)
        );
        Ok(())
    })
    .unwrap();
    assert_eq!(checkpoints, [run.game.clock.cycles]);
    let events = &run.last_timing().unwrap().events;
    assert_eq!(events.count(TimingEvent::ActorDraw), 1);
    assert_eq!(events.count(TimingEvent::SpriteErase), 1);
    assert_eq!(
        events
            .work
            .iter()
            .filter(|work| matches!(work, Work::BeginSound(_)))
            .count(),
        2
    );
    assert_eq!(events.count(TimingEvent::GraphicsCycles), 0);
    assert_eq!(events.count(TimingEvent::SoundCycles), 0);
    for key in [None, None, Some(0xa0)] {
        run.step(key).unwrap();
    }
    let trace = run.trace().unwrap();
    assert_eq!(trace.rounds[0].start_delta, 0);
    assert_eq!(replay(&trace).unwrap(), trace);
    let mut wrong = trace;
    wrong.final_delta += 100;
    assert!(replay(&wrong).is_err());
}

#[test]
fn falling_defers_input_and_landing_keeps_ordered_sound_work() {
    let mut run = run(&[(5, 14, Tile::Player), (25, 15, Tile::Gold)]);
    run.step(Some(0xcc)).unwrap();
    let interval = run.last_timing().unwrap();
    assert_eq!(run.game.player.motion, lode_trace::Motion::Falling);
    assert_eq!(interval.events.count(TimingEvent::Input), 0);
    assert_eq!(
        interval
            .events
            .work
            .iter()
            .filter(|work| matches!(work, Work::Note(_)))
            .count(),
        2
    );
    assert!(
        interval
            .events
            .work
            .iter()
            .any(|work| matches!(work, Work::Note(_)))
    );
    for _ in 0..5 {
        run.step(None).unwrap();
    }
    assert_eq!(
        run.last_timing().unwrap().events.count(TimingEvent::Input),
        1
    );
    assert_eq!(
        run.last_timing()
            .unwrap()
            .events
            .work
            .iter()
            .filter(|work| matches!(work, Work::Note(_)))
            .count(),
        1
    );
    assert!(
        run.game
            .speaker
            .edges
            .windows(2)
            .all(|pair| pair[0].cycle <= pair[1].cycle)
    );
}

// Application transitions advance audio and presentation while gameplay time
// stays at the terminal boundary. Reload drawing adds no simulated delay.
#[test]
fn death_and_reload_stop_the_gameplay_clock() {
    let mut run = run(&[(5, 15, Tile::Player), (25, 15, Tile::Gold)]);
    run.step(Some(0x81)).unwrap();
    let before = run.game.clock.cycles;
    run.step(None).unwrap();
    let interval = run.last_timing().unwrap();
    assert_eq!(interval.phase, Phase::Died);
    assert_eq!(run.game.clock.cycles, before);
    assert!(run.last_model_cycles().is_none());
    assert!(run.game.clock.output_cycles() > before);
    assert!(
        run.game
            .speaker
            .edges
            .iter()
            .all(|edge| edge.cycle <= run.game.clock.output_cycles())
    );
    let trace = run.trace().unwrap();
    assert_eq!(replay(&trace).unwrap(), trace);
}

#[test]
fn untraced_play_keeps_only_the_latest_event_record() {
    let mut run = run(&[(5, 15, Tile::Player), (25, 15, Tile::Gold)]);
    run.disable_tracing().unwrap();
    run.step(Some(0xcc)).unwrap();
    assert!(run.last_model_cycles().is_some_and(|cycles| cycles > 0));
    assert_eq!(
        run.last_timing()
            .unwrap()
            .events
            .count(TimingEvent::ActorDraw),
        1
    );
    run.step(Some(0xa0)).unwrap();
    assert_eq!(
        run.last_timing()
            .unwrap()
            .events
            .count(TimingEvent::ActorDraw),
        0
    );
    assert!(run.trace().is_err());
}

#[test]
fn pause_entry_records_its_boundary_and_polling_remains_model_work() {
    let mut run = run(&[(5, 15, Tile::Player), (25, 15, Tile::Gold)]);
    run.step(Some(0x9b)).unwrap();
    assert_eq!(run.game.session.phase, Phase::Paused);
    assert!(run.last_model_cycles().is_some_and(|cycles| cycles > 0));
    assert_eq!(
        run.last_timing().unwrap().events.count(TimingEvent::Pause),
        1
    );
    run.step(None).unwrap();
    assert_eq!(
        run.last_timing().unwrap().events.count(TimingEvent::Poll),
        1
    );
    assert_eq!(
        run.last_timing().unwrap().events.count(TimingEvent::Pause),
        0
    );
    let trace = run.trace().unwrap();
    assert_eq!(replay(&trace).unwrap(), trace);
}
