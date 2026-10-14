use lode_trace::Trace;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

struct Files(PathBuf);

impl Files {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("lode-{name}-{}", std::process::id()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Files {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn command(files: &Files, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lode-port"))
        .args(args)
        .current_dir(&files.0)
        .env("SDL_VIDEODRIVER", "dummy")
        .env("SDL_AUDIODRIVER", "dummy")
        .output()
        .unwrap()
}

#[test]
fn command_line_capture_replay_and_negative_timing_validation() {
    let files = Files::new("cli-capture");
    let capture = command(
        &files,
        &[
            "capture",
            "trace.json",
            "--rounds",
            "120",
            "--input",
            "0:cc",
            "--input",
            "40:ca",
            "--input",
            "80:d5",
        ],
    );
    assert!(
        capture.status.success(),
        "{}",
        String::from_utf8_lossy(&capture.stderr)
    );
    let replay = command(&files, &["replay", "trace.json"]);
    assert!(
        replay.status.success(),
        "{}",
        String::from_utf8_lossy(&replay.stderr)
    );
    assert!(String::from_utf8_lossy(&replay.stdout).contains("Validated 120 rounds"));
    let mut trace =
        Trace::from_json(&fs::read_to_string(files.0.join("trace.json")).unwrap()).unwrap();
    trace.rounds[3].start_delta += 1;
    fs::write(files.0.join("wrong.json"), trace.to_json().unwrap()).unwrap();
    let failure = command(&files, &["replay", "wrong.json"]);
    assert!(!failure.status.success());
    assert!(String::from_utf8_lossy(&failure.stderr).contains("round 3: replay mismatch"));
}

#[test]
fn sdl_application_trace_reexecutes_without_sdl() {
    let files = Files::new("cli-sdl");
    let live = command(&files, &["play", "--rounds", "8", "--trace", "live.json"]);
    assert!(
        live.status.success(),
        "{}",
        String::from_utf8_lossy(&live.stderr)
    );
    let replay = command(&files, &["replay", "live.json"]);
    assert!(
        replay.status.success(),
        "{}",
        String::from_utf8_lossy(&replay.stderr)
    );
}

#[test]
fn live_validation_requires_a_trace_destination() {
    let files = Files::new("cli-validation-arguments");
    let result = command(&files, &["play", "--validate-mame"]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("requires --trace FILE"));
}

#[test]
#[ignore = "requires MAME and Apple IIe enhanced ROMs"]
fn live_validation_drains_the_final_interval_and_preserves_replay() {
    let files = Files::new("cli-live-validation");
    let result = command(
        &files,
        &[
            "play",
            "--validate-mame",
            "--trace",
            "live.json",
            "--rounds",
            "12",
        ],
    );
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(String::from_utf8_lossy(&result.stdout).contains("Validated 12 intervals"));
    assert!(command(&files, &["replay", "live.json"]).status.success());
}
