//! Playable recordings carry effective Apple II commands without expected state.

use crate::actors::{Command, DigSide, Direction, Position};
use crate::devices::keyboard::{Control, LevelNavigation, RETURN};
use crate::session::{Configuration, Run};
use crate::state::Game;
use lode_trace::{Facing, Input, Phase};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// Scheduler and player history survive reloads. These values make each database
// recording independently playable from its untouched starting board.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttemptSeed {
    pub score: u32,
    pub lives_used: u32,
    pub attempt: u32,
    pub difficulty: u8,
    pub resurrection_column: u8,
    pub guard_phase: u8,
    pub guard_facings: [Facing; 5],
    pub player_facing: Facing,
    pub falling_sound: bool,
    pub falling_pitch: u8,
    pub sound_enabled: bool,
    pub frame_period: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recording {
    version: u8,
    pub configuration: Configuration,
    pub rounds: usize,
    inputs: BTreeMap<usize, u8>,
    #[serde(skip)]
    capture: Capture,
}

// Capture follows the command that Apple II playback would retain. An input
// rejected now can still become useful later, before another command replaces it.
#[derive(Clone, Debug, Default)]
struct Capture {
    movement: Option<Direction>,
    command_round: Option<usize>,
    attempt: Option<u32>,
}

pub(crate) struct RecordingBoundary {
    phase: Phase,
    attempt: u32,
    position: Position,
    eligible: bool,
    movable: [bool; 4],
}

impl RecordingBoundary {
    pub(crate) fn new(game: &Game) -> Self {
        Self {
            phase: game.session.phase,
            attempt: game.session.attempt,
            position: game.player.position,
            eligible: game.session.phase == Phase::Playing
                && game.player.digging.is_none()
                && !game.player_should_fall(),
            movable: [
                Direction::Left,
                Direction::Right,
                Direction::Up,
                Direction::Down,
            ]
            .map(|direction| game.player_can_move(direction)),
        }
    }

    fn allows(&self, direction: Direction) -> bool {
        self.movable[match direction {
            Direction::Left => 0,
            Direction::Right => 1,
            Direction::Up => 2,
            Direction::Down => 3,
        }]
    }
}

impl Recording {
    pub fn new(configuration: Configuration) -> Self {
        Self {
            version: 2,
            configuration,
            rounds: 0,
            inputs: BTreeMap::new(),
            capture: Capture::default(),
        }
    }

    // Import existing sparse recordings verbatim. Live capture uses gameplay
    // effects to select commands before serializing the same versioned format.
    pub fn push(&mut self, inputs: &[Input]) -> Result<(), String> {
        let key = observed_key(inputs)?;
        if let Some(key) = key {
            self.inputs.insert(self.rounds, key);
        }
        self.rounds = self.rounds.checked_add(1).ok_or("recording is too long")?;
        Ok(())
    }

    pub fn key(&self, round: usize) -> Option<u8> {
        self.inputs.get(&round).copied()
    }

    // Trace observations remain exact. Playable capture keeps only commands
    // needed to reproduce player actions with Apple's persistent-command rules.
    pub fn capture(&mut self, run: &Run) -> Result<(), String> {
        let Some(boundary) = run.recording_boundary() else {
            return Ok(());
        };
        let key = observed_key(run.last_inputs())?;
        let game = &run.game;
        if key.and_then(LevelNavigation::from_key).is_some() {
            if boundary.attempt != game.session.attempt {
                self.push(run.last_inputs())?;
                self.capture = Capture::default();
            }
            return Ok(());
        }
        if matches!(boundary.phase, Phase::Died | Phase::LevelCleared) {
            self.capture = Capture::default();
            return Ok(());
        }
        if self.capture.attempt != Some(boundary.attempt) {
            self.capture = Capture {
                attempt: Some(boundary.attempt),
                ..Capture::default()
            };
        }
        let round = self.rounds;
        self.push(&[])?;

        // Paused polls consume host keys without moving the world. Only the
        // resume command is needed; discarded controls cannot become movement.
        if boundary.phase == Phase::Paused {
            if game.session.phase == Phase::Playing {
                self.inputs.insert(round, Control::Pause as u8);
            }
            return Ok(());
        }
        if !boundary.eligible {
            return Ok(());
        }
        let control = key.and_then(Control::from_key).filter(|control| {
            matches!(
                control,
                Control::Pause | Control::Sacrifice | Control::EndGame | Control::SkipLevel
            )
        });
        if let Some(code) = key
            && Control::from_key(code).is_none()
            && code != RETURN
        {
            self.capture.command_round = Some(round);
        }
        if control == Some(Control::Pause) {
            self.inputs.insert(round, Control::Pause as u8);
            return Ok(());
        }

        // A successful dig clears the persistent movement command itself.
        // Failed digs need only stop a previous direction if it could still move.
        if let Some(dig) = game.player.digging {
            self.inputs.insert(
                round,
                match dig.side {
                    DigSide::Left => 0xd5,
                    DigSide::Right => 0xcf,
                },
            );
            self.capture.movement = None;
        } else {
            let moved = boundary.position.cell != game.player.position.cell
                || boundary.position.x() != game.player.position.x()
                || boundary.position.y() != game.player.position.y();
            let movement = match game.player.command {
                Command::Move(direction) if moved => Some(direction),
                _ => None,
            };
            let needed = match movement {
                Some(direction) if self.capture.movement != Some(direction) => {
                    Some((Some(direction), movement_code(direction)))
                }
                None if self
                    .capture
                    .movement
                    .is_some_and(|direction| boundary.allows(direction)) =>
                {
                    Some((None, 0xa0))
                }
                _ => None,
            };
            if let Some((movement, code)) = needed {
                // A death/skip command can also execute retained movement. Keep
                // its prerequisite at the earlier poll that established it.
                let index = if control.is_some() {
                    self.capture
                        .command_round
                        .ok_or("missing retained recording command")?
                } else {
                    round
                };
                self.inputs.insert(index, code);
                self.capture.movement = movement;
            }
        }
        if let Some(control) = control {
            self.inputs.insert(round, control as u8);
        }
        Ok(())
    }

    // Parsing rejects unsupported formats and input outside the explicit endpoint.
    pub fn from_json(text: &str) -> Result<Self, String> {
        let recording: Self = serde_json::from_str(text).map_err(|error| error.to_string())?;
        if recording.version != 2 {
            return Err("unsupported input recording version".into());
        }
        if recording
            .inputs
            .keys()
            .any(|&round| round >= recording.rounds)
        {
            return Err("input outside recording".into());
        }
        if recording.inputs.values().any(|&key| key < 0x80) {
            return Err("input codes must have the high bit set".into());
        }
        Ok(recording)
    }

    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|error| error.to_string())
    }
}

fn movement_code(direction: Direction) -> u8 {
    match direction {
        Direction::Left => 0xca,
        Direction::Right => 0xcc,
        Direction::Up => 0xc9,
        Direction::Down => 0xcb,
    }
}

pub fn observed_key(inputs: &[Input]) -> Result<Option<u8>, String> {
    match inputs {
        [] => Ok(None),
        [input] if input.observation == 0 && input.key >= 0x80 => Ok(Some(input.key)),
        _ => Err("unsupported input observation".into()),
    }
}
