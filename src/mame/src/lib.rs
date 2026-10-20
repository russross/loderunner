//! Input-only execution against the selected Apple II machine baseline.

use lode_trace::apple2::{Apple2Observer, Sample};
use lode_trace::{Change, Observer, Phase, Round, Snapshot, Trace};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
const ARCHIVE: &str = "sources/Lode Runner (4am crack).zip";
const SOURCE: &str = "apple2-reference-113db1c16951717259c9ae5e128958b29de0634d";

#[derive(Clone, Debug, Deserialize)]
pub struct LevelRecord(pub Vec<u8>);

#[derive(Clone, Debug, Deserialize)]
pub struct Configuration {
    pub levels: Vec<LevelRecord>,
    pub start: usize,
    pub wait_on_level_start: bool,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Tolerance {
    pub cycles: u64,
    pub percent: u32,
}

impl Tolerance {
    pub fn accepts(self, expected: u64, actual: u64) -> bool {
        let limit = u128::from(self.cycles) + u128::from(expected) * u128::from(self.percent) / 100;
        u128::from(expected.abs_diff(actual)) <= limit
    }
}

#[derive(Clone, Debug)]
pub struct Options {
    pub mame: PathBuf,
    pub rompath: Option<String>,
    pub pluginspath: Option<PathBuf>,
    pub work: Option<PathBuf>,
    pub timeout: Duration,
    pub gameplay: Tolerance,
    pub fixed_initialization: bool,
    pub profile_timing: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            mame: "mame".into(),
            rompath: None,
            pluginspath: None,
            work: None,
            timeout: Duration::from_secs(120),
            gameplay: Tolerance {
                cycles: 2000,
                percent: 10,
            },
            fixed_initialization: true,
            profile_timing: false,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Timing {
    pub interval: usize,
    pub phase: Phase,
    pub expected: u64,
    pub actual: u64,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub machine: &'static str,
    pub mame_version: String,
    pub initialization: &'static str,
    pub gameplay_tolerance: Tolerance,
    pub intervals: usize,
    pub timing: Vec<Timing>,
    pub work: PathBuf,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Difference {
    Initial {
        expected: Snapshot,
        actual: Snapshot,
    },
    Round {
        interval: usize,
        expected: Round,
        actual: Round,
    },
    Timing {
        interval: usize,
        expected: u64,
        actual: u64,
    },
}

// Level files are configuration inputs. Expected state transitions are
// never copied into MAME RAM or used to decide the machine's execution path.
fn prepare_disk(work: &Path, configuration: &Configuration) -> Result<(), String> {
    if configuration.levels.is_empty()
        || configuration.levels.len() > 150
        || configuration.start >= configuration.levels.len()
        || configuration
            .levels
            .iter()
            .any(|level| level.0.len() != 256)
    {
        return Err("unsupported initial level configuration".into());
    }
    let output = Command::new("python3").args(["-c",
        "import hashlib,sys,zipfile; z=zipfile.ZipFile(sys.argv[1]); b=z.read('Lode Runner (4am crack)/Lode Runner (4am crack).dsk'); assert hashlib.sha256(b).hexdigest()=='71412e9b4619291a38abc3ad60e032fcc8fba12775798f8d84aeacd108aacdf6'; sys.stdout.buffer.write(b)"])
        .arg(Path::new(ROOT).join(ARCHIVE)).output().map_err(|error| format!("disk extraction: {error}"))?;
    if !output.status.success() || output.stdout.len() != 143360 {
        return Err("cannot extract the pinned Apple II 4am disk".into());
    }
    let mut disk = output.stdout;
    // The unused portion of a configured level list is invalid board data. The
    // original loader discovers its end and wraps through its ordinary path.
    for number in 0..150 {
        let first = (3 * 16 + number) * 256;
        disk[first..first + 256].fill(0);
        if let Some(level) = configuration.levels.get(number) {
            disk[first..first + 256].copy_from_slice(&level.0);
        }
    }
    fs::write(work.join("disk.dsk"), disk).map_err(|error| error.to_string())
}

fn locate(image: &[u8], first: usize, last: usize, pattern: &[u8]) -> Result<u16, String> {
    let matches: Vec<_> = image[first..last]
        .windows(pattern.len())
        .enumerate()
        .filter_map(|(offset, bytes)| (bytes == pattern).then_some(first + offset))
        .collect();
    if matches.len() != 1 {
        return Err("source observation anchor is not unique".into());
    }
    Ok(matches[0] as u16)
}

#[derive(Serialize)]
struct Anchors {
    player_collect: u16,
    guard_collect: u16,
    guard_drop: u16,
    player_landed: u16,
    guard_contacts: Vec<u16>,
    player_hole_contact: u16,
}

fn anchors() -> Result<Anchors, String> {
    // Occurrence anchors identify committed source paths, including labels that
    // the disassembly did not name. Ambiguous instruction sequences are errors.
    let image = fs::read(Path::new(ROOT).join("sources/lode_runner_reveng/main.bin"))
        .map_err(|error| error.to_string())?;
    Ok(Anchors {
        player_collect: locate(&image, 0x6b9d, 0x6bf4, &[0x46, 0x94, 0xc6, 0x93])?,
        guard_collect: locate(&image, 0x74f7, 0x753e, &[0xa9, 0xff, 0x38, 0xe5, 0x53])?,
        guard_drop: locate(&image, 0x753e, 0x7574, &[0xa9, 0x07, 0x91, 0x08])?,
        player_landed: locate(&image, 0x6584, 0x658f, &[0xa9, 0x64, 0xa2, 0x08])?,
        guard_contacts: image[0x6cdb..0x70d8]
            .windows(2)
            .enumerate()
            .filter_map(|(i, b)| (b == [0x46, 0x9a]).then_some((i + 0x6cdb) as u16))
            .collect(),
        player_hole_contact: locate(&image, 0x7641, 0x767a, &[0x46, 0x9a])?,
    })
}

#[derive(Serialize)]
struct AdapterConfiguration<'a> {
    work: &'a Path,
    start: usize,
    wait_on_level_start: bool,
    fixed_initialization: bool,
    profile_timing: bool,
    symbols: BTreeMap<String, u16>,
    anchors: Anchors,
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let text = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, text).map_err(|error| error.to_string())?;
    fs::rename(temporary, path).map_err(|error| error.to_string())
}

struct Machine {
    child: Child,
    work: PathBuf,
    deadline: Instant,
}

// Resolve PATH before changing the child's working directory. Packaged MAME
// installs its Lua modules beside the installation's share directory.
fn executable(path: &Path) -> Result<PathBuf, String> {
    if path.components().count() > 1 {
        return std::path::absolute(path).map_err(|error| error.to_string());
    }
    Ok(std::env::var_os("PATH")
        .into_iter()
        .flat_map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
        .map(|directory| directory.join(path))
        .find_map(|candidate| candidate.canonicalize().ok())
        .unwrap_or_else(|| path.to_owned()))
}

fn installed_plugins(mame: &Path) -> Option<PathBuf> {
    let installation = mame.canonicalize().ok()?;
    let directory = installation.parent()?;
    [
        directory.parent()?.join("share/mame/plugins"),
        directory.join("plugins"),
    ]
    .into_iter()
    .find(|path| path.join("json/init.lua").is_file())
}

impl Machine {
    fn start(configuration: &Configuration, options: &Options) -> Result<Self, String> {
        let work = if let Some(work) = &options.work {
            work.clone()
        } else {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|error| error.to_string())?
                .as_nanos();
            Path::new(ROOT)
                .join("artifacts/mame-validation")
                .join(format!("{nonce}-{}", std::process::id()))
        };
        if work.exists()
            && fs::read_dir(&work)
                .map_err(|error| error.to_string())?
                .next()
                .is_some()
        {
            return Err("validation work directory must be empty".into());
        }
        fs::create_dir_all(&work).map_err(|error| error.to_string())?;
        let work = fs::canonicalize(work).map_err(|error| error.to_string())?;
        if work.to_string_lossy().contains(['"', '\\', '\n', '\r']) {
            return Err(
                "validation work path cannot contain quotes, backslashes, or newlines".into(),
            );
        }
        // Every run owns its disk, firmware side files, protocol, and output.
        // An existing nonempty directory cannot be reused or overwritten.
        prepare_disk(&work, configuration)?;
        let adapter = AdapterConfiguration {
            work: &work,
            start: configuration.start,
            wait_on_level_start: configuration.wait_on_level_start,
            fixed_initialization: options.fixed_initialization,
            profile_timing: options.profile_timing,
            symbols: serde_json::from_str(include_str!(
                "../../../sources/analysis/apple2-symbols.json"
            ))
            .map_err(|error| error.to_string())?,
            anchors: anchors()?,
        };
        write_json(&work.join("configuration.json"), &adapter)?;
        let log = File::create(work.join("mame.log")).map_err(|error| error.to_string())?;
        let mame = executable(&options.mame)?;
        let mut command = Command::new(&mame);
        // Native machine execution is unthrottled. The adapter stops at source
        // boundaries; host waits between control files never charge CPU cycles.
        command
            .args([
                "apple2ee",
                "-video",
                "none",
                "-sound",
                "none",
                "-debug",
                "-debugger",
                "none",
                "-nothrottle",
                "-skip_gameinfo",
                "-seconds_to_run",
                "3600",
                "-autoboot_script",
            ])
            .arg(Path::new(ROOT).join("src/mame/adapter.lua"))
            .arg("-flop1")
            .arg(work.join("disk.dsk"));
        for (flag, directory) in [
            ("-cfg_directory", "cfg"),
            ("-nvram_directory", "nvram"),
            ("-diff_directory", "diff"),
            ("-state_directory", "states"),
        ] {
            command.arg(flag).arg(work.join(directory));
        }
        if let Some(rompath) = &options.rompath {
            // Explicit relative ROM paths are relative to the caller, not artifacts.
            let rompath = rompath
                .split(';')
                .map(|path| {
                    if path.starts_with('$') || path.starts_with('~') {
                        Ok(path.to_owned())
                    } else {
                        std::path::absolute(path)
                            .map(|path| path.to_string_lossy().into_owned())
                            .map_err(|error| error.to_string())
                    }
                })
                .collect::<Result<Vec<_>, String>>()?
                .join(";");
            command.arg("-rompath").arg(rompath);
        }
        let plugins = options
            .pluginspath
            .clone()
            .or_else(|| installed_plugins(&mame));
        if let Some(plugins) = plugins {
            command
                .arg("-pluginspath")
                .arg(std::path::absolute(plugins).map_err(|error| error.to_string())?);
        }
        let child = command
            .env("SDL_VIDEODRIVER", "dummy")
            .env("SDL_AUDIODRIVER", "dummy")
            .env("LODE_VALIDATION_CONFIG", work.join("configuration.json"))
            .current_dir(&work)
            .stdout(Stdio::from(
                log.try_clone().map_err(|error| error.to_string())?,
            ))
            .stderr(Stdio::from(log))
            .spawn()
            .map_err(|error| format!("MAME launch: {error}"))?;
        Ok(Self {
            child,
            work,
            deadline: Instant::now() + options.timeout,
        })
    }

    fn wait_file(&mut self, name: &str) -> Result<PathBuf, String> {
        let path = self.work.join(name);
        loop {
            let error_path = self.work.join("error.json");
            if error_path.exists() {
                return Err(format!(
                    "MAME adapter failed: {} ({})",
                    fs::read_to_string(error_path).map_err(|error| error.to_string())?,
                    self.work.display()
                ));
            }
            if path.exists() {
                return Ok(path);
            }
            if Instant::now() >= self.deadline {
                return Err(format!(
                    "MAME validation timed out; inspect {}",
                    self.work.display()
                ));
            }
            if let Some(status) = self.child.try_wait().map_err(|error| error.to_string())? {
                return Err(format!(
                    "MAME exited with {status}; inspect {}",
                    self.work.join("mame.log").display()
                ));
            }
            thread::sleep(Duration::from_millis(2));
        }
    }

    fn sample(&mut self, index: usize) -> Result<Sample, String> {
        let path = self.wait_file(&format!("sample-{index}.json"))?;
        serde_json::from_slice(&fs::read(path).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())
    }

    fn command(&self, index: usize, key: Option<u8>, stop: bool, save: bool) -> Result<(), String> {
        #[derive(Serialize)]
        struct Control {
            key: Option<u8>,
            stop: bool,
            save: bool,
        }
        write_json(
            &self.work.join(format!("command-{index}.json")),
            &Control { key, stop, save },
        )
    }

    fn finish(&mut self, index: usize, save: bool) -> Result<(), String> {
        self.command(index, None, true, save)?;
        self.wait_file("done.json")?;
        let status = loop {
            if let Some(status) = self.child.try_wait().map_err(|error| error.to_string())? {
                break status;
            }
            if Instant::now() >= self.deadline {
                return Err("MAME did not exit after stop".into());
            }
            thread::sleep(Duration::from_millis(2));
        };
        if !status.success() {
            return Err(format!(
                "MAME exited with {status}; inspect {}",
                self.work.join("mame.log").display()
            ));
        }
        if save && !self.work.join("failure.sta").exists() {
            return Err(format!(
                "MAME did not save failure state; inspect {}",
                self.work.display()
            ));
        }
        Ok(())
    }
}

impl Drop for Machine {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub fn validate(expected: &Trace, options: &Options) -> Result<Report, String> {
    validate_stream(expected, options, &mut || Ok(None))
}

/// Continue a trace with committed intervals supplied while the port runs.
/// Each duration closes its interval; the snapshot makes the retained trace replayable.
pub fn validate_stream(
    initial: &Trace,
    options: &Options,
    next: &mut dyn FnMut() -> Result<Option<(Round, u64, Snapshot)>, String>,
) -> Result<Report, String> {
    let mut expected = initial.clone();
    expected.validate().map_err(|error| error.to_string())?;
    if expected.header.source != SOURCE {
        return Err("unsupported trace source identity".into());
    }
    let configuration: Configuration = serde_json::from_str(&expected.header.configuration)
        .map_err(|error| format!("trace configuration: {error}"))?;
    for round in &expected.rounds {
        if round.inputs.len() > 1
            || round
                .inputs
                .first()
                .is_some_and(|input| input.observation != 0 || input.key < 128)
        {
            return Err(format!(
                "interval {}: unsupported keyboard observations",
                round.number
            ));
        }
    }
    let mut machine = Machine::start(&configuration, options)?;
    // Keep the submitted trace for reproduction. The Lua adapter reads only its
    // configuration and keyboard/stop commands, never this comparison oracle.
    fs::write(
        machine.work.join("trace.json"),
        expected.to_json().map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    let mut decoder = Apple2Observer::default();
    let initial = machine.sample(0)?;
    let observed = decoder
        .observe(&initial)
        .map_err(|error| error.to_string())?;
    if observed.snapshot != expected.header.initial {
        write_json(
            &machine.work.join("difference.json"),
            &Difference::Initial {
                expected: expected.header.initial.clone(),
                actual: observed.snapshot,
            },
        )?;
        machine.finish(0, true)?;
        return Err(format!(
            "initial machine state differs; inspect {}",
            machine.work.display()
        ));
    }
    let mut observer =
        Observer::new(expected.header.initial.clone()).map_err(|error| error.to_string())?;
    let mut previous = initial;
    let mut timing = Vec::new();
    let mut index = 0;
    loop {
        if index == expected.rounds.len() {
            let Some((round, duration, snapshot)) = next()? else {
                break;
            };
            // Idle port time does not consume the machine-operation timeout.
            machine.deadline = Instant::now() + options.timeout;
            expected.rounds.push(round);
            expected.final_delta = duration;
            expected.final_state = snapshot;
            expected.validate().map_err(|error| error.to_string())?;
            fs::write(
                machine.work.join("trace.json"),
                expected.to_json().map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
        }
        let round = &expected.rounds[index];
        let transition = matches!(previous.phase, Phase::Died | Phase::LevelCleared);
        if round.inputs.len() > 1
            || round
                .inputs
                .first()
                .is_some_and(|input| input.observation != 0 || input.key < 128)
        {
            return Err(format!(
                "interval {index}: unsupported keyboard observations"
            ));
        }
        // Reexecute one interval from its input, then derive fresh changes from
        // machine observations rather than applying the submitted transitions.
        machine.command(
            index,
            round.inputs.first().map(|input| input.key),
            false,
            false,
        )?;
        let sample = machine.sample(index + 1)?;
        let observation = decoder
            .observe(&sample)
            .map_err(|error| error.to_string())?;
        let actual = observer
            .finish_round(
                previous.cycle,
                observation.snapshot,
                observation.inputs,
                observation.occurrences,
            )
            .map_err(|error| error.to_string())?;
        let mut logical_expected = round.clone();
        logical_expected.start_delta = actual.start_delta;
        if !transition && logical_expected != actual {
            write_json(
                &machine.work.join("difference.json"),
                &Difference::Round {
                    interval: index,
                    expected: round.clone(),
                    actual: actual.clone(),
                },
            )?;
            machine.finish(index + 1, true)?;
            return Err(format!(
                "interval {index}: {}; inspect {}",
                describe_difference(round, &actual),
                machine.work.display()
            ));
        }
        let expected_cycles = expected
            .rounds
            .get(index + 1)
            .map_or(expected.final_delta, |next| next.start_delta);
        // Start deltas close the preceding interval. Check the final duration
        // explicitly so a mismatch at the end cannot escape timing validation.
        let actual_cycles = sample
            .cycle
            .checked_sub(previous.cycle)
            .ok_or("machine clock moved backwards")?;
        if !transition {
            timing.push(Timing {
                interval: index,
                phase: previous.phase,
                expected: expected_cycles,
                actual: actual_cycles,
            });
        }
        if !transition && !options.gameplay.accepts(expected_cycles, actual_cycles) {
            write_json(
                &machine.work.join("difference.json"),
                &Difference::Timing {
                    interval: index,
                    expected: expected_cycles,
                    actual: actual_cycles,
                },
            )?;
            machine.finish(index + 1, true)?;
            return Err(format!(
                "interval {index}: timing differs ({expected_cycles} port, {actual_cycles} MAME cycles); inspect {}",
                machine.work.display()
            ));
        }
        previous = sample;
        index += 1;
    }
    machine.finish(expected.rounds.len(), false)?;
    let completed: serde_json::Value = serde_json::from_slice(
        &fs::read(machine.work.join("done.json")).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    let report = Report {
        machine: "apple2ee/w65c02",
        mame_version: completed["version"]
            .as_str()
            .ok_or("missing MAME version")?
            .into(),
        initialization: if options.fixed_initialization {
            "fixed-v2"
        } else {
            "4am-loader"
        },
        intervals: expected.rounds.len(),
        gameplay_tolerance: options.gameplay,
        timing,
        work: machine.work.clone(),
    };
    write_json(&report.work.join("report.json"), &report)?;
    Ok(report)
}

fn describe_difference(expected: &Round, actual: &Round) -> String {
    if expected.inputs != actual.inputs {
        return format!(
            "observed inputs differ: {:?} / {:?}",
            expected.inputs, actual.inputs
        );
    }
    if expected.episodes != actual.episodes {
        return format!(
            "occurrences differ: {:?} / {:?}",
            expected.episodes, actual.episodes
        );
    }
    for (old, new) in expected.changes.iter().zip(&actual.changes) {
        if old != new {
            return match (old, new) {
                (
                    Change::Actor {
                        id,
                        after: expected,
                        ..
                    },
                    Change::Actor { after: actual, .. },
                ) => format!("{id:?} differs: {expected:?} / {actual:?}"),
                _ => format!("state changes differ: {old:?} / {new:?}"),
            };
        }
    }
    "state change counts differ".into()
}
