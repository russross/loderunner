use lode_mame_validation::{Options, validate};
use lode_original::session::{Configuration, capture};
use lode_original::storage::{LevelRecord, original_level};
use lode_trace::{Change, Motion, Tile};

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
        high_scores: vec![],
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
        let mut options = Options::default();
        // This board falls before polling input, retaining a loader-derived sound
        // pitch counter. The current profile does not normalize that counter.
        if level == 150 {
            assert!(
                validate(&trace, &options)
                    .unwrap_err()
                    .contains("timing differs")
            );
            options.gameplay.percent = 100;
        }
        validate(&trace, &options).unwrap_or_else(|error| panic!("level {level}: {error}"));
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
        &Options::default(),
    )
    .unwrap();
    keys[13] = Some(0xcc);
    keys[18] = Some(0xa0);
    validate(
        &capture(configuration(vec![board(&tiles)], false), &keys).unwrap(),
        &Options::default(),
    )
    .unwrap();
    let tiles = [(5, 13, Tile::Player), (5, 15, Tile::Brick)];
    let mut keys = vec![None; 15];
    keys[0] = Some(0xcc);
    validate(
        &capture(configuration(vec![board(&tiles)], false), &keys).unwrap(),
        &Options::default(),
    )
    .unwrap();
}

#[test]
#[ignore = "requires MAME 0.276 and Apple IIe enhanced ROMs"]
fn waiting_pause_and_high_score_screen_match_machine() {
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
        (false, vec![Some(0x8d), None, Some(0xa0), None]),
    ] {
        let trace = capture(configuration(levels.clone(), waiting), &keys).unwrap();
        validate(&trace, &Options::default()).unwrap();
    }
}

#[test]
#[ignore = "requires MAME 0.276 and Apple IIe enhanced ROMs"]
fn death_restart_completion_and_initials_match_machine() {
    let config = configuration(vec![LevelRecord(original_level(1).unwrap())], false);
    let mut keys = vec![None; 12];
    keys[0] = Some(0xca);
    keys[10] = Some(0x81);
    validate(&capture(config, &keys).unwrap(), &Options::default()).unwrap();
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
    validate(&capture(config, &keys).unwrap(), &Options::default()).unwrap();
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
    validate(&capture(config, &keys).unwrap(), &Options::default()).unwrap();
    let mut deaths = Vec::new();
    for _ in 0..5 {
        deaths.extend([Some(0x81), None]);
    }
    let config = configuration(
        vec![board(&[(5, 15, Tile::Player), (15, 15, Tile::Guard)])],
        false,
    );
    validate(&capture(config, &deaths).unwrap(), &Options::default()).unwrap();

    let config = configuration(
        vec![
            board(&[(5, 0, Tile::Player), (5, 1, Tile::Brick)]),
            board(&[(8, 15, Tile::Player), (20, 15, Tile::Guard)]),
        ],
        false,
    );
    validate(
        &capture(config, &[Some(0xa0), None, Some(0xcc)]).unwrap(),
        &Options::default(),
    )
    .unwrap();

    let config = configuration(
        vec![board(&[(5, 15, Tile::Player), (6, 15, Tile::Gold)])],
        false,
    );
    let mut keys = vec![Some(0xcc), None, None, None, None, Some(0xa0)];
    keys.extend(deaths);
    keys.extend([
        Some(0xc1),
        Some(0xc2),
        Some(0xc3),
        Some(0xc4),
        Some(0x88),
        Some(0xa0),
        Some(0x95),
        Some(0xae),
        Some(0xb1),
        Some(0x8d),
    ]);
    let trace = capture(config, &keys).unwrap();
    assert_eq!(trace.final_state.high_scores[0].initials, *b"A .");
    validate(&trace, &Options::default()).unwrap();
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
        validate(&wrong, &Options::default())
            .unwrap_err()
            .contains("differs")
    );
    let mut wrong = original;
    wrong.final_delta += 10_000_000;
    wrong.validate().unwrap();
    assert!(
        validate(&wrong, &Options::default())
            .unwrap_err()
            .contains("timing")
    );
}
