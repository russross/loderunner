use crate::actors::Command;
use crate::board::Level;
use crate::devices::keyboard::{Control, LevelNavigation};
use crate::event_timing::{Model, TimingEvent, TimingInterval};
use crate::player::Journal;
use crate::sound::{NoteWork, render_note};
use crate::state::Game;
use crate::storage::{LevelRecord, original_level};
use crate::timing::NTSC_CPU_HZ;
use crate::transition::{
    BLINK_HALF_PERIOD_CYCLES, BONUS_DURATION_US, BONUS_NOTE_US, BONUS_SCORE_US, DEATH_DURATION_US,
    DEATH_NOTES, IRIS_DURATION_CYCLES, IRIS_FRAMES, VisualEffect,
};
use lode_trace::{FORMAT_VERSION, Header, Input, Observer, Phase, Round, Snapshot, Trace};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Configuration {
    pub levels: Vec<LevelRecord>,
    pub start: usize,
    pub wait_on_level_start: bool,
}

impl Configuration {
    pub fn original(level: u8) -> Result<Self, String> {
        if !(1..=150).contains(&level) {
            return Err("level must be between 1 and 150".into());
        }
        let levels = (1..=150)
            .map(|number| original_level(number).map(LevelRecord))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            levels,
            start: usize::from(level - 1),
            wait_on_level_start: true,
        })
    }

    fn decode_levels(&self) -> Result<Vec<Level>, String> {
        if self.levels.is_empty() || self.levels.len() > 150 || self.start >= self.levels.len() {
            return Err("invalid session level configuration".into());
        }
        self.levels.iter().map(LevelRecord::decode).collect()
    }
}

pub struct Run {
    pub game: Game,
    configuration: Configuration,
    levels: Vec<Level>,
    level_index: usize,
    header: Header,
    observer: Observer,
    rounds: Vec<Round>,
    last_start: u64,
    tracing: bool,
    last_inputs: Vec<Input>,
    last_timing: Option<TimingInterval>,
    last_model_cycles: Option<u64>,
    timing_model: Model,
    waiting_started: u64,
}

// Ready waits are host interaction. Trace boundaries describe a board ready for
// gameplay without retaining its blinking phase. Starting commands are observed
// through normal gameplay rather than recorded as ready-screen interaction.
fn trace_snapshot(game: &Game) -> Snapshot {
    let mut snapshot = game.snapshot();
    if snapshot.session.phase == Phase::Waiting {
        snapshot.session.phase = Phase::Playing;
    }
    snapshot
}

impl Run {
    pub fn new(configuration: Configuration, paced: bool) -> Result<Self, String> {
        let levels = configuration.decode_levels()?;
        let index = configuration.start;
        let mut game = Game::load_level(&levels[index], (index + 1) as u8, paced);
        if configuration.wait_on_level_start {
            game.session.phase = Phase::Waiting;
        }
        let initial = trace_snapshot(&game);
        let header = Header {
            version: FORMAT_VERSION,
            source: "apple2-reference-113db1c16951717259c9ae5e128958b29de0634d".into(),
            configuration: serde_json::to_string(&configuration)
                .map_err(|error| error.to_string())?,
            initial: initial.clone(),
        };
        Ok(Self {
            game,
            level_index: index,
            configuration,
            levels,
            observer: Observer::new(initial).map_err(|error| error.to_string())?,
            header,
            rounds: vec![],
            last_start: 0,
            tracing: true,
            last_inputs: Vec::new(),
            last_timing: None,
            last_model_cycles: None,
            timing_model: Model::calibrated(),
            waiting_started: 0,
        })
    }

    // Each interval commits one coherent snapshot. Transition intervals occur
    // before another gameplay round, so death and completion cannot disappear.
    pub fn step(&mut self, delivery: Option<u8>) -> Result<(), String> {
        self.step_with_checkpoint(delivery, &mut |_| Ok(()))
    }

    pub fn step_with_checkpoint(
        &mut self,
        delivery: Option<u8>,
        checkpoint: &mut dyn FnMut(&mut Game) -> Result<(), String>,
    ) -> Result<(), String> {
        if let Some(navigation) = delivery.and_then(LevelNavigation::from_key) {
            return self.navigate(navigation, checkpoint);
        }
        // The first key ends the host wait and enters the ordinary gameplay
        // latch in this same step, preserving commands and deferred observation.
        if self.game.session.phase == Phase::Waiting && delivery.is_some() {
            self.start_ready_level();
        }
        if self.game.session.phase == Phase::Waiting {
            self.last_inputs.clear();
            self.last_timing = None;
            self.last_model_cycles = None;
            let elapsed = self.game.clock.output_cycles() - self.waiting_started;
            self.game.visual_effect = if (elapsed / BLINK_HALF_PERIOD_CYCLES).is_multiple_of(2) {
                VisualEffect::None
            } else {
                let (sprite, half_x, y) = self.game.player_sprite_coordinates();
                VisualEffect::HidePlayer { sprite, half_x, y }
            };
            self.game.clock.advance_presentation(NTSC_CPU_HZ / 200);
            checkpoint(&mut self.game)?;
            self.game.clock.checkpoint();
            return Ok(());
        }
        self.last_start = self.game.clock.cycles;
        let phase = self.game.session.phase;
        let transition = matches!(phase, Phase::Died | Phase::LevelCleared);
        self.game.clock.begin_interval();
        if phase == Phase::Playing {
            self.game.clock.events.add(TimingEvent::Round, 1);
        }
        if !matches!(phase, Phase::Died | Phase::LevelCleared)
            && let Some(code) = delivery
        {
            self.game.keyboard.deliver(code);
        }
        let mut journal = Journal::new();
        let inputs = if transition {
            self.transition(checkpoint)?;
            Vec::new()
        } else {
            self.advance_phase(phase, &mut journal)?
        };

        // Boundary events describe the completed phase before model evaluation.
        // All queued graphics and audio belong to this single committed interval.
        match self.game.session.phase {
            Phase::Paused if phase == Phase::Playing => {
                self.game.clock.events.add(TimingEvent::Pause, 1)
            }
            Phase::Died if phase == Phase::Playing => {
                self.game.clock.events.add(TimingEvent::Death, 1)
            }
            Phase::LevelCleared if phase == Phase::Playing => {
                self.game.clock.events.add(TimingEvent::Completion, 1)
            }
            _ => (),
        }
        self.game
            .clock
            .events
            .add(TimingEvent::Input, inputs.len() as u64);
        let interval = TimingInterval {
            phase,
            end_phase: self.game.session.phase,
            events: self.game.clock.finish_events(),
        };
        if !transition {
            self.game
                .clock
                .commit(&self.timing_model, &interval, &mut self.game.speaker);
        }
        self.last_model_cycles = (!transition).then_some(self.game.clock.cycles - self.last_start);

        // Submit committed audio and graphics before pacing. Observation records
        // the same boundary for capture, replay and external machine validation.
        checkpoint(&mut self.game)?;
        self.game.clock.checkpoint();
        self.last_timing = Some(interval);
        self.last_inputs = inputs.clone();
        if self.tracing {
            self.rounds.push(
                self.observer
                    .finish_round(self.last_start, trace_snapshot(&self.game), inputs, journal)
                    .map_err(|error| error.to_string())?,
            );
        }
        Ok(())
    }

    fn navigate(
        &mut self,
        navigation: LevelNavigation,
        checkpoint: &mut dyn FnMut(&mut Game) -> Result<(), String>,
    ) -> Result<(), String> {
        self.last_start = self.game.clock.cycles;
        let phase = self.game.session.phase;
        self.game.clock.begin_interval();
        let target = match navigation {
            LevelNavigation::Previous => self.level_index.checked_sub(1),
            LevelNavigation::Next => {
                (self.level_index + 1 < self.levels.len()).then_some(self.level_index + 1)
            }
        };

        // Selection never marks the player dead or awards completion points.
        // Endpoint requests leave the current board and ready/pause state intact.
        if let Some(target) = target {
            self.game.transitioning = true;
            self.game.sound.clear();
            self.iris(false, checkpoint)?;
            self.level_index = target;
            self.game.session.level = (target + 1) as u8;
            self.game.session.lives_used = 1;
            self.reload()?;
            self.game.clock.begin_interval();
            self.iris(true, checkpoint)?;
            self.game.transitioning = false;
            self.game.visual_effect = VisualEffect::None;
            self.waiting_started = self.game.clock.output_cycles();
        }

        // Only the application request and new board checkpoint are retained.
        // Navigation and its animation contribute no gameplay events or cycles.
        let inputs = vec![Input {
            observation: 0,
            key: navigation as u8,
        }];
        self.last_inputs = inputs.clone();
        self.last_model_cycles = None;
        self.last_timing = Some(TimingInterval {
            phase,
            end_phase: self.game.session.phase,
            events: self.game.clock.finish_events(),
        });
        if self.tracing {
            self.rounds.push(
                self.observer
                    .finish_round(
                        self.last_start,
                        trace_snapshot(&self.game),
                        inputs,
                        Vec::new(),
                    )
                    .map_err(|error| error.to_string())?,
            );
        }
        Ok(())
    }

    // Phase dispatch advances gameplay, a transition or one bounded input poll.
    // It returns only keys actually observed, including a key retained in the latch.
    fn advance_phase(&mut self, phase: Phase, journal: &mut Journal) -> Result<Vec<Input>, String> {
        if phase == Phase::Playing {
            let inputs = self.game.play_round(journal)?;
            return Ok(inputs);
        }
        let code = self.game.keyboard.observe();
        match (phase, code) {
            (Phase::Paused, Some(code)) if Control::from_key(code) == Some(Control::Pause) => {
                self.game.session.phase = Phase::Playing;
            }
            _ => (),
        }
        self.game.clock.events.add(TimingEvent::Poll, 1);
        Ok(code
            .into_iter()
            .map(|key| Input {
                observation: 0,
                key,
            })
            .collect())
    }

    fn transition(
        &mut self,
        checkpoint: &mut dyn FnMut(&mut Game) -> Result<(), String>,
    ) -> Result<(), String> {
        self.game.transitioning = true;
        if self.game.session.phase == Phase::LevelCleared {
            self.game.session.level = self.game.session.level.wrapping_add(1);
            self.level_index += 1;
            self.game.session.lives_used = 1;
            self.game.put_status();
            self.game.sound.clear();
            let start = self.game.clock.output_cycles();
            for (step, &score_us) in BONUS_SCORE_US.iter().enumerate() {
                let end_us = BONUS_SCORE_US
                    .get(step + 1)
                    .copied()
                    .unwrap_or(BONUS_DURATION_US);
                self.game.add_score(100);
                for &note_us in &BONUS_NOTE_US[step * 3..step * 3 + 3] {
                    let onset = start + note_us * NTSC_CPU_HZ / 1_000_000;
                    let note = NoteWork {
                        enabled: self.game.sound.enabled,
                        duration: 6,
                        pitch: (15 - step as u8) << 4,
                        low_counter: 1,
                    };
                    render_note(note, |offset| self.game.speaker.access(onset + offset));
                }
                // Submit each score step with its three notes as one small audio
                // batch. Preparation and redraw costs never enter the trace clock.
                let target = start + end_us * NTSC_CPU_HZ / 1_000_000;
                self.game
                    .clock
                    .advance_presentation(target - self.game.clock.output_cycles());
                debug_assert!(end_us > score_us);
                checkpoint(&mut self.game)?;
                self.game.clock.checkpoint();
            }
        } else {
            self.game.session.lives_used = self
                .game
                .session
                .lives_used
                .checked_add(1)
                .ok_or("life-used counter overflow")?;
            if self.game.skip_level {
                self.level_index += 1;
                self.game.session.lives_used = 1;
            }
            self.game.put_status();
            self.game.sound.clear();
            let start = self.game.clock.output_cycles();
            for (index, note) in DEATH_NOTES.iter().enumerate() {
                let onset = start + note.onset_us * NTSC_CPU_HZ / 1_000_000;
                render_note(
                    NoteWork {
                        enabled: self.game.sound.enabled,
                        duration: note.duration,
                        pitch: note.pitch,
                        low_counter: (DEATH_NOTES.len() - index) as u8,
                    },
                    |offset| self.game.speaker.access(onset + offset),
                );
                let end_us = DEATH_NOTES
                    .get(index + 1)
                    .map_or(DEATH_DURATION_US, |next| next.onset_us);
                let target = start + end_us * NTSC_CPU_HZ / 1_000_000;
                self.game
                    .clock
                    .advance_presentation(target - self.game.clock.output_cycles());
                checkpoint(&mut self.game)?;
                self.game.clock.checkpoint();
            }
            // The final empty sound call retains the landing-whistle state across
            // reloads, without adding transition work to the gameplay model.
            if self.game.falling_sound {
                self.game.falling_pitch = self.game.falling_pitch.wrapping_add(1);
            }
        }
        self.iris(false, checkpoint)?;
        if self.level_index >= self.configuration.levels.len() {
            self.level_index = 0;
            self.game.difficulty += 1;
        }
        self.reload()?;
        self.game.clock.begin_interval();
        self.iris(true, checkpoint)?;
        self.game.transitioning = false;
        self.game.visual_effect = VisualEffect::None;
        self.waiting_started = self.game.clock.output_cycles();
        Ok(())
    }

    fn iris(
        &mut self,
        opening: bool,
        checkpoint: &mut dyn FnMut(&mut Game) -> Result<(), String>,
    ) -> Result<(), String> {
        for step in 1..=IRIS_FRAMES {
            let frame = if opening { step } else { IRIS_FRAMES - step };
            self.game.visual_effect = VisualEffect::Iris { frame };
            let duration = step * IRIS_DURATION_CYCLES / IRIS_FRAMES
                - (step - 1) * IRIS_DURATION_CYCLES / IRIS_FRAMES;
            self.game.clock.advance_presentation(duration);
            checkpoint(&mut self.game)?;
            self.game.clock.checkpoint();
        }
        Ok(())
    }

    fn reload(&mut self) -> Result<(), String> {
        // Board loading runs at host speed and preserves session preferences.
        self.game.session.attempt = self
            .game
            .session
            .attempt
            .checked_add(1)
            .ok_or("attempt counter overflow")?;
        self.game.reload_board(&self.levels[self.level_index]);
        self.game.session.phase = if self.configuration.wait_on_level_start {
            Phase::Waiting
        } else {
            Phase::Playing
        };
        self.game.put_status();
        Ok(())
    }

    pub fn start_ready_level(&mut self) {
        if self.game.session.phase == Phase::Waiting {
            self.game.session.phase = Phase::Playing;
            self.game.visual_effect = VisualEffect::None;
            self.game.player.command = Command::Stop;
            self.game.keyboard.acknowledge();
        }
    }

    pub fn present_initial(
        &mut self,
        checkpoint: &mut dyn FnMut(&mut Game) -> Result<(), String>,
    ) -> Result<(), String> {
        self.game.transitioning = true;
        self.iris(true, checkpoint)?;
        self.game.visual_effect = VisualEffect::None;
        self.game.transitioning = false;
        self.waiting_started = self.game.clock.output_cycles();
        Ok(())
    }

    pub fn trace(&self) -> Result<Trace, String> {
        if !self.tracing {
            return Err("trace capture was disabled for this run".into());
        }
        let trace = Trace {
            header: self.header.clone(),
            rounds: self.rounds.clone(),
            final_delta: self.game.clock.cycles - self.last_start,
            final_state: trace_snapshot(&self.game),
        };
        trace.validate().map_err(|error| error.to_string())?;
        Ok(trace)
    }

    /// Publish only the newest committed interval without copying trace history.
    pub fn validation_interval(&self) -> Option<(Round, u64, Snapshot)> {
        self.rounds.last().map(|round| {
            (
                round.clone(),
                self.game.clock.cycles - self.last_start,
                trace_snapshot(&self.game),
            )
        })
    }

    pub fn last_timing(&self) -> Option<&TimingInterval> {
        self.last_timing.as_ref()
    }

    /// Keys consumed at the latest boundary, including a previously latched key.
    pub fn last_inputs(&self) -> &[Input] {
        &self.last_inputs
    }

    pub fn last_model_cycles(&self) -> Option<u64> {
        self.last_model_cycles
    }

    pub fn disable_tracing(&mut self) -> Result<(), String> {
        if !self.rounds.is_empty() {
            return Err("trace capture must be disabled before execution".into());
        }
        self.tracing = false;
        Ok(())
    }
}

pub fn capture(configuration: Configuration, deliveries: &[Option<u8>]) -> Result<Trace, String> {
    let mut run = Run::new(configuration, false)?;
    for &delivery in deliveries {
        run.step(delivery)?;
    }
    run.trace()
}

pub fn replay(expected: &Trace) -> Result<Trace, String> {
    expected.validate().map_err(|error| error.to_string())?;
    let configuration: Configuration =
        serde_json::from_str(&expected.header.configuration).map_err(|error| error.to_string())?;
    let mut run = Run::new(configuration, false)?;
    if run.header != expected.header {
        return Err("replay initial configuration or state differs".into());
    }
    for round in &expected.rounds {
        run.start_ready_level();
        if round.inputs.len() > 1
            || round
                .inputs
                .first()
                .is_some_and(|input| input.observation != 0)
        {
            return Err(format!(
                "round {}: unsupported input observation",
                round.number
            ));
        }
        run.step(round.inputs.first().map(|input| input.key))?;
        let actual = run.rounds.last().ok_or("replay produced no round")?;
        if actual != round {
            return Err(format!(
                "round {}: replay mismatch\nexpected: {round:?}\nactual: {actual:?}",
                round.number
            ));
        }
    }
    let actual = run.trace()?;
    if actual.final_state != expected.final_state || actual.final_delta != expected.final_delta {
        return Err("replay terminal state or duration differs".into());
    }
    Ok(actual)
}
