use crate::player::Journal;
use crate::sound::Note;
use crate::state::Game;
use crate::storage::{LevelRecord, original_level};
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
            Game::load_level(&record.0, 1, false)?;
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
        let mut journal = Journal::new();
        let mut inputs = Vec::new();
        match self.game.session.phase {
            Phase::HighScores | Phase::Initials => {
                if let Some(code) = delivery {
                    self.game.keyboard.deliver(code);
                }
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
                self.game.clock.advance(5000);
            }
            Phase::Died | Phase::LevelCleared => self.transition(checkpoint)?,
            Phase::Waiting | Phase::Paused | Phase::GameOver => {
                if let Some(code) = delivery {
                    self.game.keyboard.deliver(code);
                }
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
                            self.game.command = code;
                            self.game.command_lr = code;
                        }
                        self.game.session.phase = Phase::Playing;
                    }
                }
                // A wait interval represents a bounded group of the original
                // polling loops rather than every unsuccessful keyboard read.
                self.game.clock.advance(5000);
            }
            Phase::Playing => {
                if let Some(code) = delivery {
                    self.game.keyboard.deliver(code);
                }
                self.game.clock.advance(200);
                inputs = self.game.move_player(&mut journal)?;
                if self.game.session.phase == Phase::HighScores {
                    self.game.show_high_scores();
                    self.high_score_until = self.game.clock.cycles + 5_000_000;
                }
                if self.game.alive && self.game.session.phase == Phase::Playing {
                    self.game.play_sound_section();
                    checkpoint(&mut self.game)?;
                    if self.game.gold_count == 0 {
                        self.game.enable_exit_ladders();
                    }
                    if self.game.player.row == 0
                        && self.game.player.y_adjust == 2
                        && matches!(self.game.gold_count, 0 | 0xff)
                    {
                        self.game.session.phase = Phase::LevelCleared;
                    } else {
                        self.game.guard_resurrections()?;
                        self.game.resurrection_col = (self.game.resurrection_col + 1) % 28;
                        self.game.clock.advance(690);
                        self.game.handle_hole_timers(&mut journal)?;
                        if self.game.alive {
                            self.game.play_sound_section();
                            checkpoint(&mut self.game)?;
                            self.game.move_guards(&mut journal)?;
                        }
                    }
                }
                if !self.game.alive {
                    self.game.session.phase = Phase::Died;
                }
            }
        }
        self.game.clock.advance(self.game.hires.take_cycles());
        self.game.clock.checkpoint();
        checkpoint(&mut self.game)?;
        if self.tracing {
            self.rounds.push(
                self.observer
                    .finish_round(self.last_start, self.game.snapshot(), inputs, journal)
                    .map_err(|error| error.to_string())?,
            );
        }
        Ok(())
    }

    fn transition(
        &mut self,
        checkpoint: &mut dyn FnMut(&mut Game) -> Result<(), String>,
    ) -> Result<(), String> {
        if self.game.session.phase == Phase::LevelCleared {
            self.game.session.level = self.game.session.level.wrapping_add(1);
            self.level_index += 1;
            self.game.session.lives = self.game.session.lives.saturating_add(1);
            for counter in (1..=15_u8).rev() {
                self.game.add_and_update_score(0, 1);
                for _ in 0..3 {
                    self.game.sound.append_note(Note {
                        duration: 6,
                        pitch: counter << 4,
                    })?;
                    self.game.play_sound_section();
                    checkpoint(&mut self.game)?;
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
            while self.game.sound.index != 0 {
                self.game.play_sound_section();
                checkpoint(&mut self.game)?;
            }
            self.game.play_sound_section();
            checkpoint(&mut self.game)?;
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
        let level = if new_game {
            (self.configuration.start + 1) as u8
        } else {
            self.game.session.level
        };
        let mut next =
            Game::load_level(&self.configuration.levels[self.level_index].0, level, false)?;
        std::mem::swap(&mut next.clock, &mut self.game.clock);
        std::mem::swap(&mut next.speaker, &mut self.game.speaker);
        // Level loading replaces positions and animations but retains direction
        // flags, including when a new game follows the game-over screen.
        next.player.facing = self.game.player.facing;
        next.guards.facings = self.game.guards.facings;
        next.guards.phase = self.game.guards.phase;
        // Falling sound carries into the next board until supported movement
        // consumes the pending landing sound and resets its pitch counter.
        next.falling_pitch = self.game.falling_pitch;
        next.falling_sound = self.game.falling_sound;
        if !new_game {
            next.session = self.game.session.clone();
            next.session.attempt += 1;
            next.session.exit_available = false;
            next.score = self.game.score;
            next.difficulty = self.game.difficulty;
            next.resurrection_col = self.game.resurrection_col;
        }
        next.sound.enabled = self.game.sound.enabled;
        next.high_scores = self.game.high_scores.clone();
        if !new_game {
            next.eligible_for_score = self.game.eligible_for_score;
        }
        next.sound.frame_period = self.game.sound.frame_period;
        next.session.phase = if self.configuration.wait_on_level_start {
            Phase::Waiting
        } else {
            Phase::Playing
        };
        self.game = next;
        self.game.put_status();
        Ok(())
    }

    fn qualify_high_score(&mut self) {
        if !self.game.eligible_for_score || self.game.session.score == 0 {
            return;
        }
        let rank = (self.game.session.level, self.game.session.score);
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
