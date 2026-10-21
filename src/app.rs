//! Player-facing pack selection, automatic recording, and attract playback.

use crate::cli::Arguments;
use crate::config::{Paths, Settings};
use crate::database::{self, Attempt, Catalog, Demo, DemoPlayback, Outcome, PlayerStore};
use crate::devices::keyboard::LevelNavigation;
use crate::recording::{AttemptSeed, Recording};
use crate::session::{Configuration, Run};
use crate::state::Game;
use lode_trace::{ActorId, Occurrence, Phase, Subject};
use std::time::{Duration, Instant};

pub const PREVIOUS_PACK: u8 = 0x84;
pub const NEXT_PACK: u8 = 0x85;
pub const DEMO: u8 = 0xc4;
pub const MOVEMENT_STYLE: u8 = 0x8b;

struct Attract {
    choices: Vec<Demo>,
    index: usize,
    playback: DemoPlayback,
    round: usize,
    live_seed: AttemptSeed,
}

pub struct Application {
    catalog: Catalog,
    player: PlayerStore,
    paths: Paths,
    settings: Settings,
    saved: Settings,
    packs: Vec<String>,
    session_started_at: i64,
    attempt: Option<Attempt>,
    attract: Option<Attract>,
    idle: Instant,
    idle_checked: bool,
    random: u64,
}

impl Application {
    pub fn open(arguments: &Arguments) -> Result<(Self, Configuration), String> {
        let paths = Paths::discover()?;
        Self::open_at(arguments, paths)
    }

    pub fn open_at(arguments: &Arguments, paths: Paths) -> Result<(Self, Configuration), String> {
        let catalog = Catalog::open(&paths.catalog)?;
        let mut settings = Settings::load(&paths.config)?;
        let packs = catalog.packs()?;
        if !packs.contains(&settings.pack) {
            return Err(format!("unknown saved level pack {}", settings.pack));
        }
        let level = if arguments.level_explicit {
            arguments.level
        } else {
            *settings.levels.get(&settings.pack).unwrap_or(&1)
        };
        if arguments.renderer_explicit {
            settings.display.renderer = arguments.renderer;
        }
        let configuration = catalog.configuration(&settings.pack, level)?;
        let player = PlayerStore::open(&paths.player)?;
        let session_started_at = database::timestamp()?;
        Ok((
            Self {
                catalog,
                player,
                paths,
                saved: settings.clone(),
                settings,
                packs,
                session_started_at,
                attempt: None,
                attract: None,
                idle: Instant::now(),
                idle_checked: false,
                random: session_started_at as u64 | 1,
            },
            configuration,
        ))
    }

    pub fn configure(&self, game: &mut Game) {
        game.host = self.settings.display.clone();
        game.sound.enabled = !self.settings.muted;
        game.sound.frame_period = self.settings.frame_period;
    }

    pub fn demo_number(&self) -> Option<i32> {
        self.attract
            .as_ref()
            .map(|attract| attract.choices[attract.index].number)
    }

    pub fn ready(&mut self) {
        self.idle = Instant::now();
        self.idle_checked = false;
    }

    fn save_preferences(&mut self, run: &Run) -> Result<(), String> {
        self.settings.display = run.game.host.clone();
        self.settings.muted = !run.game.sound.enabled;
        if self.attract.is_none() {
            self.settings.frame_period = run.game.sound.frame_period;
        }
        self.settings
            .levels
            .insert(self.settings.pack.clone(), run.level_number());
        if self.settings != self.saved {
            self.settings.save(&self.paths.config)?;
            self.saved = self.settings.clone();
        }
        Ok(())
    }

    fn flush(&mut self, outcome: Outcome) -> Result<(), String> {
        if let Some(attempt) = self.attempt.take() {
            self.player
                .save(&attempt, outcome, database::timestamp()?)?;
        }
        Ok(())
    }

    fn select_demo(
        &mut self,
        run: &mut Run,
        index: usize,
        present: &mut dyn FnMut(&mut Game) -> Result<(), String>,
    ) -> Result<(), String> {
        let attract = self.attract.as_mut().ok_or("demo mode is inactive")?;
        let playback = database::load_demo(
            &self.catalog,
            &self.player,
            &self.settings.pack,
            run.level_number(),
            &attract.choices[index],
        )?;
        let mut seed = playback.seed.clone();
        seed.sound_enabled = !self.settings.muted;
        run.game.clock.set_playback_speed(self.settings.demo_speed);
        run.select(
            playback.recording.configuration.clone(),
            Some(&seed),
            present,
        )?;
        run.start_ready_level();
        attract.index = index;
        attract.playback = playback;
        attract.round = 0;
        Ok(())
    }

    fn start_demo(
        &mut self,
        run: &mut Run,
        present: &mut dyn FnMut(&mut Game) -> Result<(), String>,
    ) -> Result<(), String> {
        self.idle_checked = true;
        let choices = database::demos(
            &self.catalog,
            &self.player,
            &self.settings.pack,
            run.level_number(),
        )?;
        let Some(index) = choices.iter().position(|demo| demo.number == 0) else {
            println!(
                "No demo solution for {} level {}",
                self.settings.pack,
                run.level_number()
            );
            return Ok(());
        };
        let playback = database::load_demo(
            &self.catalog,
            &self.player,
            &self.settings.pack,
            run.level_number(),
            &choices[index],
        )?;
        let live_seed = run.game.attempt_seed();
        self.attract = Some(Attract {
            choices,
            index,
            playback,
            round: 0,
            live_seed,
        });
        run.game.demonstrating = true;
        self.select_demo(run, index, present)
    }

    fn stop_demo(
        &mut self,
        run: &mut Run,
        present: &mut dyn FnMut(&mut Game) -> Result<(), String>,
    ) -> Result<(), String> {
        if let Some(attract) = self.attract.take() {
            run.game.demonstrating = false;
            run.game.clock.set_playback_speed(1);
            let mut seed = attract.live_seed;
            seed.sound_enabled = !self.settings.muted;
            run.select(run.recording_configuration(), Some(&seed), present)?;
            self.configure(&mut run.game);
            self.idle = Instant::now();
            self.idle_checked = false;
        }
        Ok(())
    }

    // None means an application action already consumed this host interval.
    // Gameplay deliveries, including a silent interval, remain explicitly nested.
    pub fn before(
        &mut self,
        run: &mut Run,
        live: Option<u8>,
        now: Instant,
        present: &mut dyn FnMut(&mut Game) -> Result<(), String>,
    ) -> Result<Option<Option<u8>>, String> {
        self.save_preferences(run)?;
        if self.attract.is_some() && matches!(live, Some(PREVIOUS_PACK | NEXT_PACK | 0x82 | 0x83)) {
            self.stop_demo(run, present)?;
        }
        if live == Some(MOVEMENT_STYLE) {
            run.game.host.commodore = !run.game.host.commodore;
            self.save_preferences(run)?;
            return Ok(None);
        }
        if self.attract.is_some() && matches!(live, Some(0x93 | 0x88 | 0x95)) {
            match live {
                Some(0x93) => self.settings.muted = !self.settings.muted,
                Some(0x88) => {
                    self.settings.demo_speed = self.settings.demo_speed.saturating_sub(1).max(1)
                }
                Some(0x95) => self.settings.demo_speed = (self.settings.demo_speed + 1).min(16),
                _ => (),
            }
            run.game.sound.enabled = !self.settings.muted;
            run.game.clock.set_playback_speed(self.settings.demo_speed);
            self.save_preferences(run)?;
            return Ok(None);
        }
        if let Some(attract) = &self.attract {
            if matches!(live, Some(0x87 | 0xa0)) {
                self.stop_demo(run, present)?;
                return Ok(None);
            }
            if matches!(live, Some(0xca | 0xcc)) {
                let count = attract.choices.len();
                let index = if live == Some(0xcc) {
                    (attract.index + 1) % count
                } else {
                    (attract.index + count - 1) % count
                };
                self.select_demo(run, index, present)?;
                return Ok(None);
            }
            // Recorded sound controls cannot override the shared live preference.
            let key = attract.playback.recording.key(attract.round);
            return Ok(Some(key.filter(|&key| key != 0x93)));
        }
        if live == Some(0x87) {
            return Ok(None);
        }
        // Restart is a host action even while falling, digging or paused. The
        // terminal presentation counts the sacrificed life and reloads normally.
        if live == Some(0x92) {
            self.flush(Outcome::Restart)?;
            run.game.player.alive = false;
            run.game.session.phase = Phase::Died;
            return Ok(Some(None));
        }
        if run.game.session.phase != Phase::Playing && matches!(live, Some(0x88 | 0x95 | 0x93)) {
            match live {
                Some(0x88) => {
                    run.game.sound.frame_period = (run.game.sound.frame_period + 1).min(15)
                }
                Some(0x95) => {
                    run.game.sound.frame_period = run.game.sound.frame_period.saturating_sub(1)
                }
                Some(0x93) => run.game.sound.enabled = !run.game.sound.enabled,
                _ => (),
            }
            self.save_preferences(run)?;
            return Ok(None);
        }
        if matches!(live, Some(PREVIOUS_PACK | NEXT_PACK)) {
            self.flush(Outcome::Abandoned)?;
            let index = self
                .packs
                .iter()
                .position(|name| name == &self.settings.pack)
                .ok_or("selected pack is missing")?;
            let target = if live == Some(NEXT_PACK) {
                (index + 1) % self.packs.len()
            } else {
                (index + self.packs.len() - 1) % self.packs.len()
            };
            self.settings.pack = self.packs[target].clone();
            let level = *self.settings.levels.get(&self.settings.pack).unwrap_or(&1);
            run.select(
                self.catalog.configuration(&self.settings.pack, level)?,
                None,
                present,
            )?;
            println!("Switched to {}", self.settings.pack);
            self.idle = Instant::now();
            self.idle_checked = false;
            self.save_preferences(run)?;
            return Ok(None);
        }
        if run.game.session.phase == Phase::Waiting
            && (live == Some(DEMO)
                || (!self.idle_checked
                    && now.saturating_duration_since(self.idle) >= Duration::from_secs(30)))
        {
            self.start_demo(run, present)?;
            return Ok(None);
        }
        if let Some(navigation) = live.and_then(LevelNavigation::from_key) {
            let configuration = run.recording_configuration();
            let changes = match navigation {
                LevelNavigation::Previous => configuration.start > 0,
                LevelNavigation::Next => configuration.start + 1 < configuration.levels.len(),
            };
            if changes {
                self.flush(Outcome::Abandoned)?;
            }
        } else if self.attempt.is_none()
            && (run.game.session.phase == Phase::Playing
                || (run.game.session.phase == Phase::Waiting && live.is_some()))
        {
            self.attempt = Some(Attempt {
                started_at: database::timestamp()?,
                session_started_at: self.session_started_at,
                pack: self.settings.pack.clone(),
                level: run.level_number(),
                seed: run.game.attempt_seed(),
                recording: Recording::new(run.recording_configuration()),
                elapsed_cycles: 0,
                gold: 0,
                guard_deaths: 0,
            });
        }
        Ok(Some(live))
    }

    pub fn after(
        &mut self,
        run: &mut Run,
        previous_phase: Phase,
        present: &mut dyn FnMut(&mut Game) -> Result<(), String>,
    ) -> Result<(), String> {
        if let Some(attract) = &mut self.attract {
            attract.round += 1;
            if attract.round >= attract.playback.recording.rounds
                || matches!(run.game.session.phase, Phase::Died | Phase::LevelCleared)
            {
                // A local xorshift chooses the next saved attempt without adding
                // any randomness to gameplay rules or consumed input recordings.
                self.random ^= self.random << 13;
                self.random ^= self.random >> 7;
                self.random ^= self.random << 17;
                let count = attract.choices.len();
                let index = if count == 1 {
                    0
                } else {
                    (attract.index + 1 + self.random as usize % (count - 1)) % count
                };
                self.select_demo(run, index, present)?;
            }
            return Ok(());
        }
        let navigation = run
            .last_inputs()
            .first()
            .and_then(|input| LevelNavigation::from_key(input.key))
            .is_some();
        if !navigation
            && matches!(
                previous_phase,
                Phase::Playing | Phase::Paused | Phase::Waiting
            )
            && run.last_timing().is_some()
            && let Some(attempt) = &mut self.attempt
        {
            attempt.recording.capture(run)?;
            if previous_phase != Phase::Paused {
                attempt.elapsed_cycles += run.last_model_cycles().unwrap_or(0);
            }
            for (subject, event) in run.last_events() {
                match (subject, event) {
                    (Subject::Actor(ActorId::Player), Occurrence::GoldCollected) => {
                        attempt.gold += 1
                    }
                    (
                        Subject::Actor(ActorId::Guard(_)),
                        Occurrence::Contact {
                            other: Subject::Cell(_),
                        },
                    ) => attempt.guard_deaths += 1,
                    _ => (),
                }
            }
            if matches!(run.game.session.phase, Phase::Died | Phase::LevelCleared) {
                let outcome = if run.game.session.phase == Phase::LevelCleared {
                    Outcome::Win
                } else if run.last_inputs().iter().any(|input| input.key == 0x92) {
                    Outcome::Restart
                } else {
                    Outcome::Death
                };
                self.flush(outcome)?;
            }
        }
        if run.game.session.phase == Phase::Waiting
            && (previous_phase != Phase::Waiting || navigation)
        {
            // Player-facing pack numbers wrap independently of the original
            // session counter retained by developer recordings and traces.
            run.game.session.level = run.level_number();
            run.game.put_status();
            self.idle = Instant::now();
            self.idle_checked = false;
        }
        self.save_preferences(run)
    }

    pub fn finish(&mut self, run: &Run) -> Result<(), String> {
        self.flush(Outcome::Quit)?;
        self.save_preferences(run)
    }
}
