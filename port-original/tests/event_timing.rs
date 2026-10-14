use lode_original::event_timing::{Model, TimingEvent};
use lode_original::session::{Configuration, Run, replay, replay_with_model};
use lode_original::storage::LevelRecord;
use lode_trace::{Phase, Tile};

fn run(tiles: &[(usize, usize, Tile)]) -> Run {
    let mut bytes = [0; 256];
    for &(col, row, tile) in tiles {
        let index = row * 28 + col;
        bytes[index / 2] |= (tile as u8) << ((index % 2) * 4);
    }
    Run::new(
        Configuration {
            high_scores: vec![],
            levels: vec![LevelRecord(bytes)],
            start: 0,
            wait_on_level_start: false,
        },
        false,
    )
    .unwrap()
}

// One moving actor requires an erase, a draw with collision detection, animation
// and two sound sections. Accounting remains independent of trace conversion.
#[test]
fn complete_turn_events_and_both_trace_timelines_reexecute() {
    let mut run = run(&[(5, 15, Tile::Player), (25, 15, Tile::Gold)]);
    for key in [Some(0xcc), None, None, Some(0xa0)] {
        run.step(key).unwrap();
    }
    let events = &run.timing_intervals()[0].events;
    assert_eq!(events.count(TimingEvent::ActorDraw), 1);
    assert_eq!(events.count(TimingEvent::SpriteErase), 1);
    assert_eq!(events.count(TimingEvent::PlayerDraw), 1);
    assert_eq!(events.count(TimingEvent::PlayerAnimation), 1);
    assert_eq!(events.count(TimingEvent::SoundSection), 2);
    let legacy = run.trace().unwrap();
    let clock = run.game.clock.cycles;
    let model = Model::calibrated();
    let alternative = run.trace_with_model(&model).unwrap();
    assert_eq!(run.game.clock.cycles, clock);
    assert_eq!(legacy.header, alternative.header);
    assert_eq!(legacy.final_state, alternative.final_state);
    assert_ne!(
        legacy.rounds[1].start_delta,
        alternative.rounds[1].start_delta
    );
    assert_eq!(replay(&legacy).unwrap(), legacy);
    assert_eq!(
        replay_with_model(&alternative, &model).unwrap(),
        alternative
    );
    let mut wrong = alternative;
    wrong.final_delta += 100;
    assert!(replay_with_model(&wrong, &model).is_err());
}

// Gravity bypasses input but still plays two notes. A later landing contributes
// its own note, while silent delay rounds keep the same fixed sound-cycle factor.
#[test]
fn sound_work_and_deferred_input_are_counted_once_per_turn() {
    let mut run = run(&[(5, 14, Tile::Player), (25, 15, Tile::Gold)]);
    run.step(Some(0xcc)).unwrap();
    let interval = &run.timing_intervals()[0];
    assert_eq!(interval.events.count(TimingEvent::PlayerFall), 1);
    assert_eq!(interval.events.count(TimingEvent::Input), 0);
    assert_eq!(interval.events.count(TimingEvent::Note), 2);
    assert_eq!(
        interval.legacy,
        interval.events.count(TimingEvent::SoundCycles)
            + interval.events.count(TimingEvent::GraphicsCycles)
            + 890
    );
    for _ in 0..5 {
        run.step(None).unwrap();
    }
    assert!(
        run.timing_intervals()
            .iter()
            .any(|interval| interval.events.count(TimingEvent::Input) == 1)
    );
    assert!(
        run.timing_intervals()
            .iter()
            .any(|interval| interval.events.count(TimingEvent::Note) == 1)
    );
}

#[test]
fn death_turn_is_modeled_and_reload_keeps_the_legacy_budget() {
    let mut run = run(&[(5, 15, Tile::Player), (25, 15, Tile::Gold)]);
    run.step(Some(0x81)).unwrap();
    run.step(None).unwrap();
    let intervals = run.timing_intervals();
    assert_eq!(intervals[0].events.count(TimingEvent::Death), 1);
    assert_eq!(intervals[0].end_phase, Phase::Died);
    assert_eq!(intervals[1].phase, Phase::Died);
    let model = Model::calibrated();
    assert_eq!(model.estimate(&intervals[1]), intervals[1].legacy);
    let trace = run.trace_with_model(&model).unwrap();
    assert_eq!(replay_with_model(&trace, &model).unwrap(), trace);
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
            .count(TimingEvent::PlayerDraw),
        1
    );
    run.step(Some(0xa0)).unwrap();
    assert_eq!(
        run.last_timing()
            .unwrap()
            .events
            .count(TimingEvent::PlayerDraw),
        0
    );
    assert!(run.timing_intervals().is_empty());
    assert!(run.trace().is_err());
}
