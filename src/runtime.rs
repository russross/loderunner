//! Session execution shares interval boundaries across every startup mode.

use crate::cli::{Arguments, Playback};
use crate::devices::keyboard::LevelNavigation;
use crate::presentation::RenderMode;
use crate::recording::{Recording, observed_key};
use crate::session::{Configuration, Run};
use crate::state::Game;
use lode_mame_validation::validate_stream;
use lode_trace::{Phase, Trace};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;

fn read(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))
}

fn write(path: &Path, text: &str) -> Result<(), String> {
    fs::write(path, text).map_err(|error| format!("{}: {error}", path.display()))
}

// Resolve existing files and destination parents before opening any output.
fn identity(path: &Path) -> Result<PathBuf, String> {
    if path.exists() {
        return path.canonicalize().map_err(|error| error.to_string());
    }
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    Ok(parent
        .canonicalize()
        .map_err(|error| error.to_string())?
        .join(path.file_name().ok_or("invalid output path")?))
}

pub struct Session {
    pub run: Run,
    arguments: Arguments,
    playback: Option<Recording>,
    expected: Option<Trace>,
    recording: Option<Recording>,
    end: Option<usize>,
}

impl Session {
    pub fn new(arguments: Arguments) -> Result<Self, String> {
        let source = match &arguments.playback {
            Playback::Live => None,
            Playback::Recording(path) | Playback::Trace(path) => Some(identity(path)?),
        };
        let mut paths = source.into_iter().collect::<Vec<_>>();
        for path in [&arguments.record, &arguments.trace].into_iter().flatten() {
            let path = identity(path)?;
            if paths.contains(&path) {
                return Err("recording, trace, and playback paths must be distinct".into());
            }
            paths.push(path);
        }

        // Playback carries its own level configuration and observed inputs.
        let (configuration, playback, expected) = match &arguments.playback {
            Playback::Live => (Configuration::original(arguments.level)?, None, None),
            Playback::Recording(path) => {
                let playback = Recording::from_json(&read(path)?)?;
                (playback.configuration.clone(), Some(playback), None)
            }
            Playback::Trace(path) => {
                let trace = Trace::from_json(&read(path)?).map_err(|error| error.to_string())?;
                let configuration = serde_json::from_str(&trace.header.configuration)
                    .map_err(|error| error.to_string())?;
                (configuration, None, Some(trace))
            }
        };
        let mut run = Run::new(configuration.clone(), arguments.display && arguments.paced)?;
        if let Some(expected) = &expected
            && run.trace()?.header != expected.header
        {
            return Err("verification initial configuration or state differs".into());
        }
        let recording = arguments
            .record
            .as_ref()
            .map(|_| Recording::new(configuration));
        if !arguments.validate {
            run.disable_tracing()?;
        }
        let endpoint = playback
            .as_ref()
            .map(|recording| recording.rounds)
            .or_else(|| expected.as_ref().map(|trace| trace.rounds.len()));
        let end = match (endpoint, arguments.rounds) {
            (Some(end), Some(limit)) => Some(end.min(limit)),
            (end, limit) => end.or(limit),
        };
        Ok(Self {
            run,
            arguments,
            playback,
            expected,
            recording,
            end,
        })
    }

    pub fn displayed(&self) -> bool {
        self.arguments.display
    }

    pub fn render_mode(&self) -> RenderMode {
        self.arguments.renderer
    }

    // A host polls once per interval and submits committed graphics/audio before pacing.
    pub fn execute(
        mut self,
        mut poll: impl FnMut(&mut Game) -> Result<(bool, Option<u8>), String>,
        mut present: impl FnMut(&mut Game) -> Result<(), String>,
    ) -> Result<(), String> {
        let (sender, worker) = if self.arguments.validate {
            let initial = self.run.trace()?;
            let options = self.arguments.mame.clone();
            let (sender, receiver) = mpsc::channel();
            let worker = thread::spawn(move || {
                validate_stream(&initial, &options, &mut || Ok(receiver.recv().ok()))
            });
            (Some(sender), Some(worker))
        } else {
            (None, None)
        };

        let mut round = 0;
        let mut gameplay_round = 0;
        self.run.game.clock.start_pacing();
        let result = (|| -> Result<(), String> {
            self.run.present_initial(&mut present)?;
            while self.end.is_none_or(|end| {
                if self.expected.is_some() {
                    round < end
                } else {
                    gameplay_round < end
                }
            }) {
                if worker.as_ref().is_some_and(|worker| worker.is_finished()) {
                    break;
                }
                if !matches!(self.arguments.playback, Playback::Live)
                    || !self.arguments.display
                    || !self.arguments.inputs.is_empty()
                {
                    self.run.start_ready_level();
                }
                let (quit, live) = poll(&mut self.run.game)?;
                if quit {
                    break;
                }
                let delivery = if let Some(playback) = &self.playback {
                    playback.key(gameplay_round)
                } else if let Some(expected) = &self.expected {
                    observed_key(&expected.rounds[round].inputs)?
                } else {
                    self.arguments
                        .inputs
                        .binary_search_by_key(&gameplay_round, |&(index, _)| index)
                        .ok()
                        .map(|index| self.arguments.inputs[index].1)
                        .or(live)
                };
                let transition = matches!(
                    self.run.game.session.phase,
                    Phase::Died | Phase::LevelCleared
                ) && delivery.and_then(LevelNavigation::from_key).is_none();
                self.run.step_with_checkpoint(delivery, &mut present)?;
                if self.run.last_timing().is_none() {
                    continue;
                }
                if !transition && let Some(recording) = &mut self.recording {
                    recording.push(self.run.last_inputs())?;
                }
                if !transition {
                    gameplay_round += 1;
                }
                if let Some(expected) = &self.expected
                    && self
                        .run
                        .validation_interval()
                        .ok_or("missing committed interval")?
                        .0
                        != expected.rounds[round]
                {
                    return Err(format!("round {round}: verification mismatch"));
                }
                if let Some(sender) = &sender {
                    sender
                        .send(
                            self.run
                                .validation_interval()
                                .ok_or("missing committed interval")?,
                        )
                        .map_err(|_| "MAME validation stopped")?;
                }
                round += 1;
                if !self.arguments.display {
                    self.run
                        .game
                        .speaker
                        .discard_before(self.run.game.clock.output_cycles());
                }
            }
            if let Some(expected) = &self.expected
                && round == expected.rounds.len()
            {
                let actual = self.run.trace()?;
                if actual.final_state != expected.final_state
                    || actual.final_delta != expected.final_delta
                {
                    return Err("verification terminal state or duration differs".into());
                }
            }
            Ok(())
        })();

        // Close the stream and preserve completed output before reporting worker errors.
        drop(sender);
        let trace_saved = self.arguments.trace.as_ref().map(|path| {
            self.run.trace().and_then(|trace| {
                let text = trace.to_json().map_err(|error| error.to_string())?;
                write(path, &text)
            })
        });
        let recording_saved = self.recording.as_ref().map(|recording| {
            let path = self
                .arguments
                .record
                .as_ref()
                .ok_or("missing recording path")?;
            write(path, &recording.to_json()?)
        });
        let validated = worker
            .map(|worker| {
                worker
                    .join()
                    .map_err(|_| "MAME validation worker panicked".to_owned())
                    .and_then(|report| report)
            })
            .transpose();
        let errors = [trace_saved, recording_saved]
            .into_iter()
            .flatten()
            .filter_map(Result::err)
            .collect::<Vec<_>>();
        if !errors.is_empty() {
            return Err(errors.join("; "));
        }
        let validated = validated?;
        result?;
        if let Some(report) = validated {
            println!(
                "Validated {} intervals; report: {}",
                report.intervals,
                report.work.join("report.json").display()
            );
        }
        if let Some(expected) = &self.expected {
            if round == expected.rounds.len() {
                println!("Verified {round} rounds");
            } else {
                println!("Stopped after {round} rounds; verification incomplete");
            }
        }
        Ok(())
    }
}
