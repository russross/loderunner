use crate::actors::Command;
use crate::event_timing::{Model, TimingEvent, TimingInterval};
use crate::player::Journal;
use crate::score::Score;
use crate::sound::Note;
use crate::state::{Game, Progress};
use crate::storage::{LevelRecord, decode_level, original_level};
use lode_trace::{FORMAT_VERSION, Header, Input, Observer, Phase, Trace};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Configuration {
    pub high_scores: Vec<lode_trace::HighScore>,
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
            high_scores: crate::storage::original_high_scores()?,
            levels,
            start: usize::from(level - 1),
            wait_on_level_start: true,
        })
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.levels.is_empty() || self.levels.len() > 150 || self.start >= self.levels.len() {
            return Err("invalid session level configuration".into());
        }
        for record in &self.levels {
            if !decode_level(&record.0).contains(&lode_trace::Tile::Player) {
                return Err("level contains no player".into());
            }
        }
        Ok(())
    }
}

pub struct Run {
    pub game: Game,
    configuration: Configuration,
    level_index: usize,
    header: Header,
    observer: Observer,
    rounds: Vec<lode_trace::Round>,
    last_start: u64,
    tracing: bool,
    initials: Option<(usize, usize)>,
    high_score_until: u64,
    timing_intervals: Vec<TimingInterval>,
    last_timing: Option<TimingInterval>,
    last_model_cycles: Option<u64>,
    timing_model: Model,
}

impl Run {
    pub fn new(configuration: Configuration, paced: bool) -> Result<Self, String> {
        configuration.validate()?;
        let index = configuration.start;
        let mut game = Game::load_level(&configuration.levels[index].0, (index + 1) as u8, paced)?;
        game.high_scores = configuration.high_scores.clone();
        if configuration.wait_on_level_start {
            game.session.phase = Phase::Waiting;
        }
        let initial = game.snapshot();
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
            observer: Observer::new(initial).map_err(|error| error.to_string())?,
            header,
            rounds: vec![],
            last_start: 0,
            tracing: true,
            initials: None,
            high_score_until: 0,
            timing_intervals: Vec::new(),
            last_timing: None,
            last_model_cycles: None,
            timing_model: Model::calibrated(),
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
        self.last_start = self.game.clock.cycles;
        let phase = self.game.session.phase;
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
        let mut inputs = Vec::new();
        match phase {
            Phase::HighScores | Phase::Initials => {
                let code = self.game.keyboard.read();
                if code & 0x80 != 0 {
                    self.game.keyboard.clear_strobe();
                    inputs.push(Input {
                        observation: 0,
                        key: code,
                    });
                    if self.game.session.phase == Phase::Initials {
                        self.enter_initial(code);
                    } else {
                        self.game.restore_background();
                        self.game.session.phase = Phase::Playing;
                    }
                } else if self.game.session.phase == Phase::HighScores
                    && self.game.clock.cycles >= self.high_score_until
                {
                    self.game.restore_background();
                    self.game.session.phase = Phase::Playing;
                }
                self.game.clock.events.add(TimingEvent::Poll, 1);
            }
            Phase::Died | Phase::LevelCleared => self.transition()?,
            Phase::Waiting | Phase::Paused | Phase::GameOver => {
                let code = self.game.keyboard.read_mmio(0xc000);
                if code & 0x80 != 0 {
                    self.game.keyboard.clear_strobe();
                    inputs.push(Input {
                        observation: 0,
                        key: code,
                    });
                    if self.game.session.phase == Phase::GameOver {
                        self.level_index = self.configuration.start;
                        self.reload(true)?;
                    } else if self.game.session.phase != Phase::Paused || code == 0x9b {
                        if self.game.session.phase == Phase::Waiting && code >= 0xa0 {
                            self.game.player.command = Command::from_key(code);
                        }
                        self.game.session.phase = Phase::Playing;
                    }
                }
                // Polls represent bounded batches of device reads. Their cost
                // belongs to the same model as movement, graphics and sound.
                self.game.clock.events.add(TimingEvent::Poll, 1);
            }
            Phase::Playing => {
                inputs = self.game.play_round(&mut journal)?;
                if self.game.session.phase == Phase::HighScores {
                    self.game.show_high_scores();
                    self.high_score_until = self.game.clock.cycles + 5_000_000;
                }
            }
        }
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
        self.game
            .clock
            .commit(&self.timing_model, &interval, &mut self.game.speaker);
        self.last_model_cycles = Some(self.game.clock.cycles - self.last_start);
        checkpoint(&mut self.game)?;
        self.game.clock.checkpoint();
        self.last_timing = Some(interval.clone());
        if self.tracing {
            self.timing_intervals.push(interval);
            self.rounds.push(
                self.observer
                    .finish_round(self.last_start, self.game.snapshot(), inputs, journal)
                    .map_err(|error| error.to_string())?,
            );
        }
        Ok(())
    }

    fn transition(&mut self) -> Result<(), String> {
        if self.game.session.phase == Phase::LevelCleared {
            self.game.session.level = self.game.session.level.wrapping_add(1);
            self.level_index += 1;
            self.game.session.lives = self.game.session.lives.saturating_add(1);
            for counter in (1..=15_u8).rev() {
                self.game.add_score(100);
                for _ in 0..3 {
                    self.game.sound.append_note(Note {
                        duration: 6,
                        pitch: counter << 4,
                    })?;
                    self.game.play_sound_section();
                }
            }
        } else {
            self.game.session.lives = self.game.session.lives.wrapping_sub(1);
            if self.game.skip_level {
                self.level_index += 1;
            }
            for (duration, pitch) in (2..=13_u8).zip((0x40..=0xf0_u8).step_by(16)) {
                self.game
                    .sound
                    .load_sound_data(&[Note { duration, pitch }, Note { duration, pitch }])?;
            }
            while !self.game.sound.is_empty() {
                self.game.play_sound_section();
            }
            self.game.play_sound_section();
            if self.game.session.lives == 0 {
                self.game.session.phase = Phase::GameOver;
                self.qualify_high_score();
                return Ok(());
            }
        }
        if self.level_index >= self.configuration.levels.len() {
            self.level_index = 0;
            self.game.difficulty += 1;
        }
        self.reload(false)
    }

    fn reload(&mut self, new_game: bool) -> Result<(), String> {
        // A new game resets progress, while another attempt retains it. Board
        // loading leaves preferences, device history and facing history in place.
        if new_game {
            self.game.session = Progress::new((self.configuration.start + 1) as u8);
            self.game.score = Score::default();
            self.game.difficulty = 0;
            self.game.resurrection_col = 0;
            self.game.eligible_for_score = true;
        } else {
            self.game.session.attempt += 1;
        }
        self.game
            .reload_board(&self.configuration.levels[self.level_index].0)?;
        self.game.session.phase = if self.configuration.wait_on_level_start {
            Phase::Waiting
        } else {
            Phase::Playing
        };
        self.game.put_status();
        Ok(())
    }

    fn qualify_high_score(&mut self) {
        if !self.game.eligible_for_score || self.game.score.decimal() == 0 {
            return;
        }
        let rank = (self.game.session.level, self.game.score.decimal());
        let index = self
            .game
            .high_scores
            .iter()
            .position(|entry| rank > (entry.level, entry.score))
            .unwrap_or(self.game.high_scores.len());
        if index >= 10 {
            return;
        }
        self.game.high_scores.insert(
            index,
            lode_trace::HighScore {
                initials: [b' '; 3],
                level: rank.0,
                score: rank.1,
            },
        );
        self.game.high_scores.truncate(10);
        self.initials = Some((index, 0));
        self.game.session.phase = Phase::Initials;
        self.game.show_high_scores();
    }

    fn enter_initial(&mut self, code: u8) {
        let Some((index, mut cursor)) = self.initials else {
            return;
        };
        if code == 0x8d {
            self.initials = None;
            self.game.session.phase = Phase::GameOver;
            return;
        }
        if code == 0x88 {
            cursor = cursor.saturating_sub(1);
        } else if code == 0x95 {
            cursor = (cursor + 1).min(2);
        } else if matches!(code & 0x7f, b'A'..=b'Z' | b'.' | b' ') {
            self.game.high_scores[index].initials[cursor] = code & 0x7f;
            cursor = (cursor + 1).min(2);
        }
        self.initials = Some((index, cursor));
        self.game.show_high_scores();
    }

    pub fn trace(&self) -> Result<Trace, String> {
        if !self.tracing {
            return Err("trace capture was disabled for this run".into());
        }
        let trace = Trace {
            header: self.header.clone(),
            rounds: self.rounds.clone(),
            final_delta: self.game.clock.cycles - self.last_start,
            final_state: self.game.snapshot(),
        };
        trace.validate().map_err(|error| error.to_string())?;
        Ok(trace)
    }

    /// Publish only the newest committed interval without copying trace history.
    pub fn validation_interval(&self) -> Option<(lode_trace::Round, u64, lode_trace::Snapshot)> {
        self.rounds.last().map(|round| {
            (
                round.clone(),
                self.game.clock.cycles - self.last_start,
                self.game.snapshot(),
            )
        })
    }

    pub fn last_timing(&self) -> Option<&TimingInterval> {
        self.last_timing.as_ref()
    }

    pub fn last_model_cycles(&self) -> Option<u64> {
        self.last_model_cycles
    }

    pub fn timing_intervals(&self) -> &[TimingInterval] {
        &self.timing_intervals
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
