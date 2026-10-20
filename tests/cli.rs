use loderunner::recording::Recording;
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
    Command::new(env!("CARGO_BIN_EXE_loderunner"))
        .args(args)
        .current_dir(&files.0)
        .env("SDL_VIDEODRIVER", "dummy")
        .env("SDL_AUDIODRIVER", "dummy")
        .output()
        .unwrap()
}

fn success(output: Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn host_timings_capture_checkpoints_and_refuse_to_overwrite() {
    let files = Files::new("cli-timings");
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_loderunner"))
            .args([
                "--display",
                "--unpaced",
                "--renderer",
                "simple",
                "--rounds",
                "2",
                "--input",
                "0:cc",
            ])
            .current_dir(&files.0)
            .env("SDL_VIDEODRIVER", "dummy")
            .env("SDL_AUDIODRIVER", "dummy")
            .env("CRT_BACKEND", "cpu")
            .env("LODERUNNER_FRAME_TIMINGS", "timings.csv")
            .output()
            .unwrap()
    };
    success(run());
    let original = fs::read_to_string(files.0.join("timings.csv")).unwrap();
    assert!(
        original
            .starts_with("checkpoint,cycles,transition,audio_ms,graphics_ms,work_ms,interval_ms\n")
    );
    let frames: Vec<_> = original
        .lines()
        .skip(1)
        .map(|line| line.split(',').collect::<Vec<_>>())
        .collect();
    assert!(frames.len() > 2);
    assert!(frames.iter().any(|frame| frame[2] == "true"));
    assert!(frames.iter().any(|frame| frame[2] == "false"));
    assert!(frames[0][6].is_empty());
    for frame in &frames {
        let audio: f64 = frame[3].parse().unwrap();
        let graphics: f64 = frame[4].parse().unwrap();
        let work: f64 = frame[5].parse().unwrap();
        assert!(work + 0.000002 >= audio + graphics);
    }
    let rejected = run();
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("frame timings"));
    assert_eq!(
        original,
        fs::read_to_string(files.0.join("timings.csv")).unwrap()
    );
}

#[test]
fn recording_replays_headlessly_and_with_display() {
    let files = Files::new("cli-recording");
    // Obsolete score files are ignored by live play as well as playback.
    fs::write(files.0.join("highscores.json"), "invalid scores").unwrap();
    success(command(
        &files,
        &[
            "--headless",
            "--rounds",
            "24",
            "--record",
            "play.input",
            "--input",
            "0:cc",
            "--input",
            "10:ca",
        ],
    ));
    let original = fs::read_to_string(files.0.join("play.input")).unwrap();
    let recording = Recording::from_json(&original).unwrap();
    assert_eq!(recording.rounds, 24);
    assert!(!original.contains("final_state"));
    assert!(!original.contains("high_scores"));
    success(command(
        &files,
        &[
            "--replay",
            "play.input",
            "--headless",
            "--record",
            "copy.input",
        ],
    ));
    assert_eq!(
        original,
        fs::read_to_string(files.0.join("copy.input")).unwrap()
    );
    success(command(
        &files,
        &["--replay", "play.input", "--display", "--unpaced"],
    ));
    assert_eq!(
        fs::read_to_string(files.0.join("highscores.json")).unwrap(),
        "invalid scores"
    );
}

#[test]
fn displayed_live_play_records_without_validation() {
    let files = Files::new("cli-sdl");
    success(command(
        &files,
        &["--rounds", "8", "--record", "live.input", "--input", "0:a0"],
    ));
    let recording =
        Recording::from_json(&fs::read_to_string(files.0.join("live.input")).unwrap()).unwrap();
    assert_eq!(recording.rounds, 8);
    success(command(&files, &["--replay", "live.input", "--headless"]));
}

#[test]
fn recordings_exclude_transitions_and_resume_inputs_at_gameplay_boundaries() {
    let files = Files::new("cli-transitions");
    success(command(
        &files,
        &[
            "--headless",
            "--rounds",
            "12",
            "--record",
            "play.input",
            "--input",
            "1:81",
            "--input",
            "5:81",
            "--input",
            "9:cc",
        ],
    ));
    let text = fs::read_to_string(files.0.join("play.input")).unwrap();
    let recording = Recording::from_json(&text).unwrap();
    assert_eq!(recording.rounds, 12);
    assert_eq!(recording.key(1), Some(0x81));
    assert_eq!(recording.key(5), Some(0x81));
    success(command(
        &files,
        &[
            "--headless",
            "--replay",
            "play.input",
            "--record",
            "copy.input",
        ],
    ));
    assert_eq!(
        text,
        fs::read_to_string(files.0.join("copy.input")).unwrap()
    );
}

#[test]
fn recording_replays_direct_level_navigation_and_endpoint_requests() {
    let files = Files::new("cli-navigation");
    success(command(
        &files,
        &[
            "--headless",
            "--rounds",
            "7",
            "--record",
            "play.input",
            "--input",
            "0:82",
            "--input",
            "1:83",
            "--input",
            "3:82",
        ],
    ));
    let text = fs::read_to_string(files.0.join("play.input")).unwrap();
    let recording = Recording::from_json(&text).unwrap();
    assert_eq!(recording.key(0), Some(0x82));
    assert_eq!(recording.key(1), Some(0x83));
    assert_eq!(recording.key(3), Some(0x82));
    success(command(
        &files,
        &[
            "--headless",
            "--replay",
            "play.input",
            "--record",
            "copy.input",
        ],
    ));
    assert_eq!(
        text,
        fs::read_to_string(files.0.join("copy.input")).unwrap()
    );
}

#[test]
fn invalid_startup_combinations_fail_before_execution() {
    let files = Files::new("cli-errors");
    for (args, message) in [
        (vec!["--headless"], "requires --rounds"),
        (vec!["--rounds", "0"], "must be positive"),
        (vec!["--level", "151"], "between 1 and 150"),
        (vec!["--headless", "--display"], "mutually exclusive"),
        (vec!["--replay", "one", "--verify", "two"], "choose one"),
        (vec!["--replay", "one", "--level", "2"], "cannot override"),
        (
            vec!["--rounds", "2", "--input", "2:cc"],
            "exceeds run length",
        ),
        (
            vec!["--input", "0:cc", "--input", "0:ca"],
            "multiple inputs",
        ),
        (vec!["--mame", "missing"], "MAME options require"),
    ] {
        let output = command(&files, &args);
        assert!(!output.status.success(), "{args:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(message),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn playback_cannot_overwrite_its_source_or_alias() {
    let files = Files::new("cli-paths");
    success(command(
        &files,
        &["--headless", "--rounds", "2", "--record", "play.input"],
    ));
    let original = fs::read(files.0.join("play.input")).unwrap();
    let output = command(
        &files,
        &[
            "--replay",
            "play.input",
            "--record",
            "./play.input",
            "--headless",
        ],
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("must be distinct"));
    assert_eq!(original, fs::read(files.0.join("play.input")).unwrap());
}

#[test]
fn validation_without_a_trace_destination_reports_machine_launch_failure() {
    let files = Files::new("cli-validation");
    let output = command(
        &files,
        &[
            "--headless",
            "--rounds",
            "2",
            "--validate",
            "--mame",
            "/nonexistent/loderunner-mame",
        ],
    );
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("MAME"), "{error}");
    assert!(!error.contains("requires --trace"), "{error}");
}

#[test]
fn trace_and_recording_are_saved_when_machine_launch_fails() {
    let files = Files::new("cli-trace-failure");
    let output = command(
        &files,
        &[
            "--headless",
            "--rounds",
            "2",
            "--trace",
            "play.trace",
            "--record",
            "play.input",
            "--mame",
            "/nonexistent/loderunner-mame",
        ],
    );
    assert!(!output.status.success());
    let trace =
        lode_trace::Trace::from_json(&fs::read_to_string(files.0.join("play.trace")).unwrap())
            .unwrap();
    let recording =
        Recording::from_json(&fs::read_to_string(files.0.join("play.input")).unwrap()).unwrap();
    assert_eq!(recording.rounds, trace.rounds.len());
}

#[test]
#[ignore = "requires MAME and Apple IIe enhanced ROMs"]
fn machine_record_trace_verify_and_playback_share_startup_options() {
    let files = Files::new("cli-machine-workflows");
    let mame = std::env::var("MAME").unwrap_or_else(|_| "mame".into());
    let rompath = std::env::var("ROMPATH").unwrap_or_default();
    let execute = |args: &[&str]| {
        let mut args = args.to_vec();
        args.extend(["--mame", &mame]);
        if !rompath.is_empty() {
            args.extend(["--rompath", &rompath]);
        }
        command(&files, &args)
    };
    success(execute(&[
        "--headless",
        "--rounds",
        "30",
        "--trace",
        "play.trace",
        "--record",
        "play.input",
        "--input",
        "0:cc",
        "--input",
        "10:ca",
        "--input",
        "20:d5",
    ]));
    let verified = execute(&["--verify", "play.trace"]);
    assert!(String::from_utf8_lossy(&verified.stdout).contains("Verified 30 rounds"));
    success(verified);
    success(execute(&[
        "--verify",
        "play.trace",
        "--display",
        "--unpaced",
    ]));
    success(execute(&[
        "--replay",
        "play.input",
        "--headless",
        "--validate",
        "--record",
        "copy.input",
    ]));
    assert_eq!(
        fs::read(files.0.join("play.input")).unwrap(),
        fs::read(files.0.join("copy.input")).unwrap()
    );
    let prefix = execute(&["--verify", "play.trace", "--rounds", "5"]);
    assert!(String::from_utf8_lossy(&prefix.stdout).contains("verification incomplete"));
    success(prefix);

    // Structural validity cannot make altered expected timing drive the game.
    let mut trace =
        lode_trace::Trace::from_json(&fs::read_to_string(files.0.join("play.trace")).unwrap())
            .unwrap();
    trace.rounds[3].start_delta += 1;
    fs::write(files.0.join("wrong.trace"), trace.to_json().unwrap()).unwrap();
    let wrong = execute(&["--verify", "wrong.trace"]);
    assert!(!wrong.status.success());
    assert!(String::from_utf8_lossy(&wrong.stderr).contains("round 3: verification mismatch"));
}

#[test]
fn renderer_selection_and_startup_command_summary_are_available() {
    let files = Files::new("cli-renderers");
    let default = command(&files, &["--rounds", "2", "--input", "0:a0"]);
    assert!(String::from_utf8_lossy(&default.stdout).contains("Renderer: crt"));
    success(default);
    for mode in ["crt", "simple"] {
        let output = command(
            &files,
            &["--rounds", "2", "--renderer", mode, "--input", "0:a0"],
        );
        let text = String::from_utf8_lossy(&output.stdout);
        assert!(text.contains("Snap to 1x - 5x"), "{text}");
        assert!(text.contains("Tab") && text.contains("Ctrl+Q"), "{text}");
        assert!(text.contains(if mode == "crt" {
            "Renderer: crt"
        } else {
            "Renderer: simple"
        }));
        success(output);
    }
    for mode in ["unknown", "classic"] {
        let invalid = command(&files, &["--renderer", mode]);
        assert!(!invalid.status.success());
        assert!(
            String::from_utf8_lossy(&invalid.stderr).contains("renderer must be crt or simple")
        );
    }
    let help = command(&files, &["--help"]);
    assert!(String::from_utf8_lossy(&help.stdout).contains("--renderer MODE"));
    success(help);
}
