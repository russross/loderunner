//! Controlled boards and held-out original levels exercise complete machine rounds.
use lode_mame_validation::{Options, Tolerance, validate};
use lode_original::event_timing::{Model, TimingInterval};
use lode_original::session::{Configuration, Run, replay_with_model};
use lode_original::storage::{LevelRecord, original_level};
use lode_trace::{Tile, Trace};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Serialize, Deserialize)]
struct Row {
    scenario: String,
    split: String,
    interval: usize,
    native: TimingInterval,
    actual: u64,
    machine_events: serde_json::Value,
}

fn board(tiles: &[(usize, usize, Tile)]) -> LevelRecord {
    let mut bytes = [0; 256];
    for &(col, row, tile) in tiles {
        let index = row * 28 + col;
        bytes[index / 2] |= (tile as u8) << ((index % 2) * 4);
    }
    LevelRecord(bytes)
}

fn write(path: &Path, value: &impl Serialize) -> Result<(), String> {
    fs::write(
        path,
        serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

fn execute(output: &Path) -> Result<(), String> {
    fs::create_dir_all(output).map_err(|e| e.to_string())?;
    let mut scenarios = Vec::new();
    // Vary positions, guard counts, support and commands independently. Boards
    // are data inputs to the original loader, rather than expected-state patches.
    for col in [2, 8, 16, 24] {
        for guards in [0, 1, 3, 5] {
            let mut tiles = vec![(col, 15, Tile::Player), (26, 15, Tile::Gold)];
            for slot in 0..guards {
                tiles.push((slot * 3, 15, Tile::Guard));
            }
            if tiles.iter().skip(2).any(|&(x, _, _)| x == col) {
                continue;
            }
            let mut keys = vec![None; 45];
            for (i, key) in [
                (0, 0xcc),
                (8, 0xa0),
                (12, 0xca),
                (20, 0xa0),
                (25, 0x93),
                (30, 0x95),
            ] {
                keys[i] = Some(key);
            }
            scenarios.push((
                format!("floor-{col}-{guards}"),
                "train",
                board(&tiles),
                keys,
            ));
        }
    }
    for (name, tiles, keys) in [
        (
            "dig",
            vec![
                (5, 14, Tile::Player),
                (4, 15, Tile::Brick),
                (5, 15, Tile::Brick),
                (6, 15, Tile::Brick),
                (25, 15, Tile::Gold),
            ],
            {
                let mut keys = vec![None; 210];
                keys[0] = Some(0xcf);
                keys[13] = Some(0xa0);
                keys
            },
        ),
        (
            "fall",
            vec![(5, 1, Tile::Player), (25, 15, Tile::Gold)],
            vec![None; 90],
        ),
        (
            "gold",
            vec![
                (5, 15, Tile::Player),
                (6, 15, Tile::Gold),
                (25, 15, Tile::Gold),
            ],
            {
                let mut keys = vec![None; 45];
                keys[0] = Some(0xcc);
                keys[12] = Some(0xa0);
                keys
            },
        ),
        (
            "ladder",
            (0..16)
                .filter(|&row| row != 14)
                .map(|row| (8, row, Tile::Ladder))
                .chain([(8, 14, Tile::Player), (25, 15, Tile::Gold)])
                .collect(),
            {
                let mut keys = vec![None; 70];
                keys[0] = Some(0xc9);
                keys[40] = Some(0xcb);
                keys
            },
        ),
    ] {
        scenarios.push((name.into(), "train", board(&tiles), keys));
    }
    scenarios.push((
        "completion".into(),
        "train",
        board(&[
            (5, 0, Tile::Player),
            (5, 1, Tile::Brick),
            (8, 0, Tile::HiddenLadder),
        ]),
        vec![None; 8],
    ));
    scenarios.push((
        "sacrifice".into(),
        "train",
        board(&[(5, 15, Tile::Player), (25, 15, Tile::Gold)]),
        vec![Some(0x81), None, Some(0x81), None, Some(0x81), None],
    ));
    // A distant guard reaches the hole shortly before it refills, exercising
    // burial and subsequent resurrection through ordinary original gameplay.
    let mut burial_tiles: Vec<_> = (0..28).map(|col| (col, 15, Tile::Brick)).collect();
    burial_tiles.extend([
        (5, 14, Tile::Player),
        (27, 14, Tile::Guard),
        (25, 10, Tile::Gold),
    ]);
    let mut burial_keys = vec![None; 250];
    burial_keys[0] = Some(0xcf);
    burial_keys[13] = Some(0xa0);
    scenarios.push(("burial".into(), "train", board(&burial_tiles), burial_keys));
    // Whole levels are held out, avoiding leakage from adjacent correlated turns.
    for level in [1, 2, 3, 5, 10, 15, 25, 40, 50, 75, 100, 125, 150] {
        let mut keys = vec![None; 120];
        for (i, key) in [
            (0, 0xcc),
            (11, 0xa0),
            (20, 0xca),
            (31, 0xd5),
            (45, 0xcf),
            (60, 0xc9),
            (75, 0xcb),
            (90, 0xa0),
        ] {
            keys[i] = Some(key);
        }
        let split = if [2, 10, 50, 150].contains(&level) {
            "validation"
        } else if [5, 40, 125].contains(&level) {
            "test"
        } else {
            "train"
        };
        scenarios.push((
            format!("level-{level}"),
            split,
            LevelRecord(original_level(level)?),
            keys,
        ));
    }
    let model = Model::calibrated();
    let measurements = output.join("measurements.json");
    let mut rows: Vec<Row> = if measurements.exists() {
        serde_json::from_str(&fs::read_to_string(&measurements).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?
    } else {
        Vec::new()
    };
    for (name, split, level, keys) in scenarios {
        if rows.iter().any(|row| row.scenario == name) {
            continue;
        }
        eprintln!("measuring {name} ({split})");
        let mut run = Run::new(
            Configuration {
                high_scores: vec![],
                levels: vec![level],
                start: 0,
                wait_on_level_start: false,
            },
            false,
        )?;
        for key in keys {
            run.step(key)?;
        }
        let trace = run.trace()?;
        let options = Options {
            profile_timing: true,
            gameplay: Tolerance {
                cycles: u64::MAX,
                percent: 0,
            },
            ..Options::default()
        };
        let report = validate(&trace, &options)?;
        write(&output.join(format!("{name}-machine.json")), &report)?;
        write(&output.join(format!("{name}-legacy.json")), &trace)?;
        write(
            &output.join(format!("{name}-model.json")),
            &run.trace_with_model(&model)?,
        )?;
        for (interval, timing) in run.timing_intervals().iter().zip(&report.timing) {
            let machine_events = serde_json::from_str(
                &fs::read_to_string(
                    report
                        .work
                        .join(format!("timing-{}.json", timing.interval + 1)),
                )
                .map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
            rows.push(Row {
                scenario: name.clone(),
                split: split.into(),
                interval: timing.interval,
                native: interval.clone(),
                actual: timing.actual,
                machine_events,
            });
        }
        write(&output.join("measurements.json"), &rows)?;
    }
    Ok(())
}

// Instrumentation refinements do not require rerunning unchanged original code.
// Reexecute recorded inputs, require exact legacy equality, and pair new native
// counts with the retained independently measured machine durations and events.
fn refresh_native(output: &Path) -> Result<(), String> {
    let mut rows: Vec<Row> = serde_json::from_str(
        &fs::read_to_string(output.join("measurements.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let mut scenarios: Vec<String> = rows.iter().map(|row| row.scenario.clone()).collect();
    scenarios.sort();
    scenarios.dedup();
    for scenario in scenarios {
        let trace = Trace::from_json(
            &fs::read_to_string(output.join(format!("{scenario}-legacy.json")))
                .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        let configuration =
            serde_json::from_str(&trace.header.configuration).map_err(|e| e.to_string())?;
        let mut run = Run::new(configuration, false)?;
        for round in &trace.rounds {
            run.step(round.inputs.first().map(|input| input.key))?;
        }
        if run.trace()? != trace {
            return Err(format!("{scenario}: legacy execution changed"));
        }
        for row in rows.iter_mut().filter(|row| row.scenario == scenario) {
            row.native = run.timing_intervals()[row.interval].clone();
        }
    }
    write(&output.join("measurements.json"), &rows)
}

fn verify_model(output: &Path) -> Result<(), String> {
    let rows: Vec<Row> = serde_json::from_str(
        &fs::read_to_string(output.join("measurements.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let mut names: Vec<String> = rows
        .iter()
        .filter(|row| row.split == "test")
        .map(|row| row.scenario.clone())
        .collect();
    names.extend([
        "dig".into(),
        "fall".into(),
        "gold".into(),
        "completion".into(),
        "sacrifice".into(),
        "burial".into(),
    ]);
    names.sort();
    names.dedup();
    let model = Model::calibrated();
    for name in names {
        eprintln!("validating event-model trace {name}");
        let trace = Trace::from_json(
            &fs::read_to_string(output.join(format!("{name}-model.json")))
                .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        replay_with_model(&trace, &model)?;
        let report = validate(&trace, &Options::default())?;
        for timing in &report.timing {
            let measured = rows
                .iter()
                .find(|row| row.scenario == name && row.interval == timing.interval)
                .ok_or_else(|| format!("{name}: missing retained machine interval"))?;
            if measured.actual != timing.actual {
                return Err(format!(
                    "{name} interval {}: profiling changed machine duration",
                    timing.interval
                ));
            }
        }
        write(
            &output.join(format!("{name}-model-validation.json")),
            &report,
        )?;
    }
    Ok(())
}

fn main() {
    let output = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "artifacts/event-timing".into());
    let result = match std::env::args().nth(2).as_deref() {
        None => execute(Path::new(&output)),
        Some("--refresh-native") => refresh_native(Path::new(&output)),
        Some("--verify-model") => verify_model(Path::new(&output)),
        Some(_) => Err("unknown timing action".into()),
    };
    if let Err(error) = result {
        eprintln!("timing measurement: {error}");
        std::process::exit(1);
    }
}
