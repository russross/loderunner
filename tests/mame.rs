use lode_mame_validation::{Options, validate};
use lode_trace::apple2::Sample;
use lode_trace::{Change, Motion, Tile};
use loderunner::session::{Configuration, Run, capture};
use loderunner::storage::{LevelRecord, original_level};
use std::fs;

// External checks accept installation paths without changing library defaults.
fn options() -> Options {
    Options {
        mame: std::env::var_os("MAME").map_or_else(|| "mame".into(), Into::into),
        rompath: std::env::var("ROMPATH")
            .ok()
            .filter(|path| !path.is_empty()),
        ..Options::default()
    }
}

#[test]
#[ignore = "requires MAME and Apple IIe enhanced ROMs"]
fn streamed_gameplay_matches_machine_through_the_final_interval() {
    let mut run = Run::new(
        configuration(vec![LevelRecord(original_level(1).unwrap())], false),
        false,
    )
    .unwrap();
    let initial = run.trace().unwrap();
    let mut supplied = 0;
    let report = lode_mame_validation::validate_stream(&initial, &options(), &mut || {
        if supplied == 20 {
            return Ok(None);
        }
        run.step(match supplied {
            0 => Some(0xcc),
            10 => Some(0xa0),
            12 => Some(0xd5),
            _ => None,
        })?;
        supplied += 1;
        Ok(run.validation_interval())
    })
    .unwrap();
    assert_eq!(report.intervals, 20);
    let retained =
        lode_trace::Trace::from_json(&fs::read_to_string(report.work.join("trace.json")).unwrap())
            .unwrap();
    assert_eq!(retained, run.trace().unwrap());
}

#[test]
#[ignore = "requires MAME and Apple IIe enhanced ROMs"]
fn streamed_intervals_stop_at_the_first_timing_mismatch() {
    let mut run = Run::new(
        configuration(vec![LevelRecord(original_level(1).unwrap())], false),
        false,
    )
    .unwrap();
    let initial = run.trace().unwrap();
    let mut supplied = 0;
    let error = lode_mame_validation::validate_stream(&initial, &options(), &mut || {
        supplied += 1;
        run.step(Some(0xcc))?;
        let (round, _, snapshot) = run.validation_interval().unwrap();
        Ok(Some((round, 1, snapshot)))
    })
    .unwrap_err();
    assert_eq!(supplied, 1);
    assert!(error.contains("interval 0: timing differs"), "{error}");
    let work = error.split("inspect ").nth(1).unwrap();
    assert!(std::path::Path::new(work).join("failure.sta").exists());
    let trace = lode_trace::Trace::from_json(
        &fs::read_to_string(std::path::Path::new(work).join("trace.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(trace.rounds.len(), 1);
}

fn board(tiles: &[(usize, usize, Tile)]) -> LevelRecord {
    let mut bytes = [0; 256];
    for &(col, row, tile) in tiles {
        let index = row * 28 + col;
        bytes[index / 2] |= (tile as u8) << ((index % 2) * 4);
    }
    LevelRecord(bytes)
}

fn configuration(levels: Vec<LevelRecord>, waiting: bool) -> Configuration {
    Configuration {
        levels,
        start: 0,
        wait_on_level_start: waiting,
    }
}

// These checks boot the pinned disk with real firmware and inject only keys.
// They are separate from portable tests because MAME and ROMs are prerequisites.
#[test]
#[ignore = "requires MAME 0.276 and Apple IIe enhanced ROMs"]
fn gameplay_and_occurrences_match_original_machine() {
    for level in [1, 2, 10, 25, 50, 100, 150] {
        let config = configuration(vec![LevelRecord(original_level(level).unwrap())], false);
        let mut keys = vec![None; 90];
        for (round, key) in [
            (0, 0xcc),
            (11, 0xa0),
            (20, 0xca),
            (31, 0xd5),
            (45, 0xcf),
            (60, 0xc9),
            (75, 0xcb),
        ] {
            keys[round] = Some(key);
        }
        let trace = capture(config, &keys).unwrap();
        validate(&trace, &options()).unwrap_or_else(|error| panic!("level {level}: {error}"));
    }
}

#[test]
#[ignore = "requires MAME 0.276 and Apple IIe enhanced ROMs"]
fn digging_refill_falling_and_delayed_input_match_machine() {
    let tiles = [
        (5, 14, Tile::Player),
        (4, 15, Tile::Brick),
        (5, 15, Tile::Brick),
        (6, 15, Tile::Brick),
    ];
    let mut keys = vec![None; 200];
    keys[0] = Some(0xcf);
    keys[13] = Some(0xa0);
    validate(
        &capture(configuration(vec![board(&tiles)], false), &keys).unwrap(),
        &options(),
    )
    .unwrap();
    keys[13] = Some(0xcc);
    keys[18] = Some(0xa0);
    validate(
        &capture(configuration(vec![board(&tiles)], false), &keys).unwrap(),
        &options(),
    )
    .unwrap();
    let tiles = [(5, 13, Tile::Player), (5, 15, Tile::Brick)];
    let mut keys = vec![None; 15];
    keys[0] = Some(0xcc);
    validate(
        &capture(configuration(vec![board(&tiles)], false), &keys).unwrap(),
        &options(),
    )
    .unwrap();
}

#[test]
#[ignore = "requires MAME 0.276 and Apple IIe enhanced ROMs"]
fn airborne_restarts_preserve_sound_state_without_reapplying_profile() {
    let config = configuration(
        vec![board(&[
            (5, 0, Tile::Player),
            (6, 1, Tile::Guard),
            (6, 2, Tile::Brick),
        ])],
        false,
    );
    let mut run = Run::new(config, false).unwrap();
    let mut sound_states = vec![(run.game.falling_pitch, run.game.falling_sound)];
    for _ in 0..100 {
        if run.game.session.lives_used >= 7 {
            let trace = run.trace().unwrap();
            let report = validate(&trace, &options()).unwrap();
            assert_eq!(report.initialization, "fixed-v2");
            // These execution counters are deliberately outside the trace model.
            // Inspect retained raw observations only after input-only validation.
            for (index, (pitch, falling)) in sound_states.into_iter().enumerate() {
                let sample: Sample = serde_json::from_str(
                    &fs::read_to_string(report.work.join(format!("sample-{index}.json"))).unwrap(),
                )
                .unwrap();
                let byte = |address: usize| {
                    u8::from_str_radix(&sample.memory[address * 2..address * 2 + 2], 16).unwrap()
                };
                assert_eq!(byte(0xa4), pitch, "sample {index}: falling pitch");
                assert_eq!(
                    byte(0x9b) == 0,
                    falling,
                    "sample {index}: pending landing sound"
                );
            }
            return;
        }
        run.step(None).unwrap();
        sound_states.push((run.game.falling_pitch, run.game.falling_sound));
    }
    panic!("airborne collisions did not reach six reloads");
}

#[test]
#[ignore = "requires MAME 0.276 and Apple IIe enhanced ROMs"]
fn waiting_pause_and_return_match_machine() {
    let levels = vec![board(&[(5, 15, Tile::Player), (20, 15, Tile::Guard)])];
    for (waiting, keys) in [
        (true, vec![None, None, Some(0xa0), Some(0xcc), None]),
        (
            false,
            vec![
                Some(0x9b),
                None,
                Some(0xa0),
                Some(0x9b),
                Some(0x93),
                Some(0x95),
                Some(0x88),
                Some(0xcc),
            ],
        ),
        (false, vec![Some(0xcc), Some(0x8d), None, Some(0xa0)]),
    ] {
        let trace = capture(configuration(levels.clone(), waiting), &keys).unwrap();
        let report = validate(&trace, &options()).unwrap();
        if keys.first() == Some(&Some(0x9b)) {
            assert_eq!(report.timing[0].expected, report.timing[0].actual);
        }
    }
}

#[test]
#[ignore = "requires MAME 0.276 and Apple IIe enhanced ROMs"]
fn gameplay_resumes_after_death_restart_and_completion_without_transition_validation() {
    let config = configuration(vec![LevelRecord(original_level(1).unwrap())], false);
    let mut keys = vec![None; 12];
    keys[0] = Some(0xca);
    keys[10] = Some(0x81);
    validate(&capture(config, &keys).unwrap(), &options()).unwrap();
    let config = configuration(vec![LevelRecord(original_level(1).unwrap())], true);
    let keys = [
        Some(0xa0),
        Some(0xca),
        None,
        Some(0x81),
        None,
        None,
        Some(0xa0),
        Some(0xcc),
        None,
    ];
    validate(&capture(config, &keys).unwrap(), &options()).unwrap();
    let config = configuration(vec![LevelRecord(original_level(1).unwrap())], false);
    let keys = [
        Some(0xcc),
        None,
        None,
        None,
        None,
        Some(0x92),
        None,
        Some(0xa0),
        Some(0xcc),
        None,
    ];
    validate(&capture(config, &keys).unwrap(), &options()).unwrap();
    let mut deaths = Vec::new();
    for _ in 0..5 {
        deaths.extend([Some(0x81), None]);
    }
    let config = configuration(
        vec![board(&[(5, 15, Tile::Player), (15, 15, Tile::Guard)])],
        false,
    );
    validate(&capture(config, &deaths).unwrap(), &options()).unwrap();

    let config = configuration(
        vec![
            board(&[(5, 0, Tile::Player), (5, 1, Tile::Brick)]),
            board(&[(8, 15, Tile::Player), (20, 15, Tile::Guard)]),
        ],
        false,
    );
    validate(
        &capture(config, &[Some(0xa0), None, Some(0xcc)]).unwrap(),
        &options(),
    )
    .unwrap();

    let config = configuration(
        vec![board(&[(5, 15, Tile::Player), (6, 15, Tile::Gold)])],
        false,
    );
    let mut keys = vec![Some(0xcc), None, None, None, None, Some(0xa0)];
    keys.extend(deaths);
    let trace = capture(config, &keys).unwrap();
    assert_eq!(trace.final_state.session.phase, lode_trace::Phase::Playing);
    assert_eq!(trace.final_state.session.lives_used, 6);
    validate(&trace, &options()).unwrap();
}

#[test]
#[ignore = "requires MAME 0.276 and Apple IIe enhanced ROMs"]
fn valid_but_false_state_and_timing_are_rejected_with_checkpoint() {
    let config = configuration(vec![board(&[(5, 15, Tile::Player)])], false);
    let original = capture(config, &[Some(0xcc)]).unwrap();
    let mut wrong = original.clone();
    for change in &mut wrong.rounds[0].changes {
        if let Change::Actor {
            after: Some(actor), ..
        } = change
        {
            actor.motion = Motion::Left;
        }
    }
    wrong.final_state.actors[0].motion = Motion::Left;
    wrong.validate().unwrap();
    assert!(
        validate(&wrong, &options())
            .unwrap_err()
            .contains("differs")
    );
    let mut wrong = original;
    wrong.final_delta += 10_000_000;
    wrong.validate().unwrap();
    assert!(validate(&wrong, &options()).unwrap_err().contains("timing"));
}

#[test]
#[ignore = "requires MAME 0.276 and Apple IIe enhanced ROMs"]
fn delayed_guard_enters_refilling_hole_and_resurrects() {
    let mut tiles: Vec<_> = (0..28).map(|col| (col, 15, Tile::Brick)).collect();
    tiles.extend([
        (5, 14, Tile::Player),
        (27, 14, Tile::Guard),
        (25, 10, Tile::Gold),
    ]);
    let mut keys = vec![None; 250];
    keys[0] = Some(0xcf);
    keys[13] = Some(0xa0);
    let trace = capture(configuration(vec![board(&tiles)], false), &keys).unwrap();
    assert_eq!(trace.final_state.session.score, 150);
    assert!(trace.rounds.iter().any(|round|round.changes.iter().any(|change|matches!(change,Change::Actor {after:Some(actor),..} if actor.life==lode_trace::Life::Resurrecting))));
    validate(&trace, &options()).unwrap();
}
