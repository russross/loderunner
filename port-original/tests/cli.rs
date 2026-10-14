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
    Command::new(env!("CARGO_BIN_EXE_lode-original"))
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
            "--event-trace",
            "events.json",
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
    let event_replay = command(&files, &["replay", "events.json", "--event-model"]);
    assert!(
        event_replay.status.success(),
        "{}",
        String::from_utf8_lossy(&event_replay.stderr)
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
