//! Compare an observed Total Recall timeline with native input execution.

use lode_trace::{Phase, Snapshot};
use loderunner::recording::{OverridePlayback, Overrides, Recording};
use loderunner::session::{Configuration, Run};
use loderunner::storage::{decode_level, original_level};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::num::NonZeroU8;
use std::path::Path;

#[derive(Deserialize)]
struct Capture {
    version: u8,
    level: u8,
    board: String,
    tile: [f64; 2],
    demo: Demo,
    ticks: Vec<Tick>,
}

#[derive(Deserialize)]
struct Demo {
    time: usize,
    #[serde(rename = "goldDrop")]
    gold: Vec<NonZeroU8>,
    #[serde(rename = "bornPos")]
    born: Vec<u8>,
}

// Source coordinates retain pixel offsets so conversion can compare physical
// positions without pretending its 9/44 vertical step equals a native fifth.
#[derive(Clone, Deserialize, Serialize)]
struct Position {
    x: u8,
    y: u8,
    #[serde(rename = "xOffset")]
    x_offset: i16,
    #[serde(rename = "yOffset")]
    y_offset: i16,
}

#[derive(Deserialize)]
struct State {
    player: Position,
}

// Dig checks expose the exact terrain mutation independently of animation ticks.
#[derive(Deserialize)]
struct DigState {
    #[serde(rename = "digTarget")]
    target: Option<DigTarget>,
}

#[derive(Deserialize)]
struct DigTarget {
    x: u8,
    y: u8,
    active: u8,
}

#[derive(Deserialize)]
struct Tick {
    tick: usize,
    before: State,
    events: Vec<Event>,
}

#[derive(Deserialize)]
#[serde(tag = "event")]
enum Event {
    #[serde(rename = "key")]
    Key { args: [u8; 1] },
    #[serde(rename = "dig-check")]
    DigCheck { before: DigState, after: DigState },
    #[serde(other)]
    Other,
}

#[derive(Serialize, Deserialize)]
struct Frame {
    round: usize,
    source_tick: Option<usize>,
    position_error: Option<f64>,
    delivery: Option<u8>,
    consumed: Option<u8>,
    snapshot: Snapshot,
    dig_stage: Option<u8>,
}

fn key(code: u8) -> Result<u8, String> {
    Ok(match code {
        37 => 0xca,
        38 => 0xc9,
        39 => 0xcc,
        40 => 0xcb,
        32 => 0xa0,
        90 => 0xd5,
        88 => 0xcf,
        _ => return Err(format!("unsupported source key {code}")),
    })
}

fn execute(
    capture: Capture,
    destination: &Path,
    timings: BTreeMap<usize, usize>,
) -> Result<(), String> {
    let report = destination.with_extension("report.json");
    if destination.exists() || report.exists() {
        return Err("recording or report destination already exists".into());
    }
    if capture.version != 1
        || capture
            .tile
            .iter()
            .any(|value| !value.is_finite() || *value <= 0.0)
    {
        return Err("unsupported capture version or invalid tile dimensions".into());
    }
    // Verify the same board before attributing divergence to timing or rules.
    let source_tiles = capture
        .board
        .chars()
        .map(|character| match character {
            ' ' => Ok(0),
            '#' => Ok(1),
            '@' => Ok(2),
            'H' => Ok(3),
            '-' => Ok(4),
            'X' => Ok(5),
            'S' => Ok(6),
            '$' => Ok(7),
            '0' => Ok(8),
            '&' => Ok(9),
            _ => Err(format!("unsupported source tile {character:?}")),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let native_tiles = decode_level(&original_level(capture.level)?).map(|tile| tile as u8);
    if source_tiles != native_tiles {
        return Err("source board differs from the native original board".into());
    }
    // Explicit experiment timings preserve source key order and reject collisions.
    let mut deliveries = BTreeMap::new();
    let mut previous_round = None;
    let mut remaining_timings = timings;
    for tick in &capture.ticks {
        for event in &tick.events {
            if let Event::Key { args: [code] } = event {
                let round = remaining_timings.remove(&tick.tick).unwrap_or(tick.tick);
                if previous_round.is_some_and(|previous| round <= previous) {
                    return Err("key timings must preserve source order without collisions".into());
                }
                deliveries.insert(round, (key(*code)?, tick));
                previous_round = Some(round);
            }
        }
    }
    if !remaining_timings.is_empty() {
        return Err("key timing refers to a source tick without a key event".into());
    }
    let configuration = Configuration::original(capture.level)?;
    let mut recording = Recording::new(configuration.clone());
    let stride = capture.demo.born.first().copied().unwrap_or(1);
    if !matches!(stride, 1 | 2) {
        return Err("invalid resurrection coordinate stride".into());
    }
    let columns = capture
        .demo
        .born
        .get(1..)
        .unwrap_or_default()
        .iter()
        .step_by(usize::from(stride))
        .map(|&column| column.try_into().map(Some))
        .collect::<Result<Vec<_>, _>>()?;
    recording.overrides = Overrides {
        gold_delays: capture.demo.gold.iter().copied().map(Some).collect(),
        resurrection_columns: columns,
    };
    let mut run = Run::new(configuration, false)?;
    run.disable_tracing()?;
    run.start_ready_level();
    run.game.recording_overrides = OverridePlayback::new(recording.overrides.clone());
    let mut frames = Vec::new();
    let early_openings = capture
        .ticks
        .iter()
        .flat_map(|tick| {
            tick.events.iter().filter_map(move |event| {
                if let Event::DigCheck { before, after } = event
                    && let (Some(before), Some(after)) = (&before.target, &after.target)
                    && before.active == 1
                    && after.active == 0
                {
                    Some((tick.tick, after.x, after.y))
                } else {
                    None
                }
            })
        })
        .collect::<Vec<_>>();
    let endpoint = previous_round
        .unwrap_or(0)
        .max(capture.demo.time)
        .checked_add(1)
        .ok_or("recording is too long")?;
    for round in 0..endpoint {
        let entry = deliveries.get(&round);
        let delivery = entry.map(|(code, _)| *code);
        // Position errors compare key-delivery anchors, not equal-numbered frames.
        let position_error = entry.map(|(_, tick)| {
            let native = run.game.player.position;
            let source = &tick.before.player;
            (f64::from(native.cell.col) + (f64::from(native.x()) - 2.0) / 5.0
                - f64::from(source.x)
                - f64::from(source.x_offset) / capture.tile[0])
                .abs()
                + (f64::from(native.cell.row) + (f64::from(native.y()) - 2.0) / 5.0
                    - f64::from(source.y)
                    - f64::from(source.y_offset) / capture.tile[1])
                    .abs()
        });
        run.step(delivery)?;
        recording.push(run.last_inputs())?;
        frames.push(Frame {
            round,
            source_tick: entry.map(|(_, tick)| tick.tick),
            position_error,
            delivery,
            consumed: run.last_inputs().first().map(|input| input.key),
            snapshot: run.game.snapshot(),
            dig_stage: run.game.player.digging.map(|dig| dig.stage()),
        });
        if matches!(run.game.session.phase, Phase::Died | Phase::LevelCleared) {
            break;
        }
    }
    fs::write(destination, recording.to_json()?).map_err(|error| error.to_string())?;
    fs::write(
        &report,
        serde_json::to_string(&frames).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    println!(
        "level={} native_rounds={} phase={:?}; {}",
        capture.level,
        recording.rounds,
        run.game.session.phase,
        report.display()
    );
    for (tick, x, y) in early_openings {
        println!("source tick {tick}: dig check opens ({x}, {y}) before animation completion");
    }
    Ok(())
}

fn main() -> std::process::ExitCode {
    let result = (|| {
        let mut args = std::env::args().skip(1);
        let usage = "usage: total_recall CAPTURE OUTPUT [--key-round SOURCE_TICK:NATIVE_ROUND]...";
        let source = args.next().ok_or(usage)?;
        let destination = args.next().ok_or(usage)?;
        let mut timings = BTreeMap::new();
        while let Some(option) = args.next() {
            if option != "--key-round" {
                return Err(usage.into());
            }
            let value = args.next().ok_or(usage)?;
            let (source, native) = value.split_once(':').ok_or(usage)?;
            if timings
                .insert(
                    source.parse().map_err(|_| usage)?,
                    native.parse().map_err(|_| usage)?,
                )
                .is_some()
            {
                return Err("duplicate source key timing".into());
            }
        }
        let capture = fs::read_to_string(source).map_err(|error| error.to_string())?;
        let capture: Capture = serde_json::from_str(&capture).map_err(|error| error.to_string())?;
        execute(capture, Path::new(&destination), timings)
    })();
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("total_recall: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Capture, Frame, execute};
    use lode_trace::{Phase, Tile};
    use loderunner::recording::Recording;
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::PathBuf;

    struct Directory(PathBuf);

    impl Drop for Directory {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    fn fixture() -> Capture {
        serde_json::from_str(include_str!(
            "../tests/fixtures/total-recall-12-events.json"
        ))
        .unwrap()
    }

    fn directory(name: &str) -> Directory {
        let path = std::env::temp_dir().join(format!("recall-{name}-{}", std::process::id()));
        fs::create_dir(&path).unwrap();
        Directory(path)
    }

    #[test]
    fn reference_first_dig_exposes_a_terrain_rule_difference() {
        let directory = directory("first-dig");
        let destination = directory.0.join("recording.json");
        let capture = fixture();
        let opening = capture
            .ticks
            .iter()
            .find(|tick| {
                tick.events.iter().any(|event| {
                    matches!(event, super::Event::DigCheck { before, after }
                if before.target.as_ref().is_some_and(|value| value.active == 1)
                && after.target.as_ref().is_some_and(|value| value.active == 0))
                })
            })
            .unwrap();
        assert_eq!(opening.tick, 61);
        execute(capture, &destination, BTreeMap::new()).unwrap();
        let recording = Recording::from_json(&fs::read_to_string(&destination).unwrap()).unwrap();
        let frames: Vec<Frame> = serde_json::from_str(
            &fs::read_to_string(destination.with_extension("report.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(recording.rounds, 73);
        assert!(frames[61].dig_stage.is_some());
        assert!(frames[62].dig_stage.is_none());
        assert_eq!(frames[62].snapshot.active[11 * 28 + 25], Tile::Brick);
        assert!(frames[62].snapshot.holes.is_empty());
        assert_eq!(frames.last().unwrap().snapshot.session.phase, Phase::Died);
    }

    #[test]
    fn timing_experiments_reject_reordered_or_nonexistent_key_events() {
        let directory = directory("invalid-timing");
        let destination = directory.0.join("recording.json");
        for timings in [BTreeMap::from([(35, 55)]), BTreeMap::from([(1, 2)])] {
            assert!(execute(fixture(), &destination, timings).is_err());
            assert!(!destination.exists());
            assert!(!destination.with_extension("report.json").exists());
        }
    }
}
