use crate::graphics::{draw_sprite, draw_sprite_at_pixels, erase_sprite_at_pixels};
use crate::player::Journal;
use crate::sound::play_note;
use crate::state::Game;
use crate::timing::{sound_delay, sound_delay1};
use lode_trace::{FORMAT_VERSION, Header, Observer, Phase, Subject, Tile, Trace};

impl Game {
    pub fn play_sound_section(&mut self) {
        let index = usize::from(self.sound.index);
        if index != 0 {
            let duration = self.sound.durations[index];
            self.clock.advance(19);
            play_note(
                &mut self.clock,
                &mut self.speaker,
                self.sound.enabled,
                self.sound.pitches[index],
                duration,
                self.sound.index,
            );
            self.sound.index -= 1;
            match self.sound.frame_period.cmp(&duration) {
                std::cmp::Ordering::Equal => self.clock.advance(28),
                std::cmp::Ordering::Less => self.clock.advance(30),
                std::cmp::Ordering::Greater => self
                    .clock
                    .advance(37 + sound_delay1(self.sound.frame_period - duration)),
            }
        } else if self.falling_sound {
            let pitch = self.falling_pitch >> 1;
            self.falling_pitch = self.falling_pitch.wrapping_add(1);
            self.clock.advance(34);
            play_note(
                &mut self.clock,
                &mut self.speaker,
                self.sound.enabled,
                pitch,
                self.sound.frame_period + 2,
                0,
            );
            self.clock.advance(8);
        } else {
            self.clock
                .advance(29 + sound_delay(self.sound.frame_period));
        }
        self.clock.checkpoint();
    }

    // A background brick survives a hole. Refill changes active occupancy and both
    // graphics pages, while the logical background retains its original terrain.
    pub fn handle_hole_timers(&mut self, journal: &mut Journal) -> Result<(), String> {
        for slot in (0..31).rev() {
            if self.holes.timers[slot] == 0 {
                continue;
            }
            let col = self.holes.cols[slot];
            let row = self.holes.rows[slot];
            let index = usize::from(row) * 28 + usize::from(col);
            self.holes.timers[slot] -= 1;
            match self.holes.timers[slot] {
                20 | 10 => {
                    let sprite = if self.holes.timers[slot] == 20 {
                        0x37
                    } else {
                        0x38
                    };
                    draw_sprite(&mut self.hires, 1, sprite, col, row);
                    erase_sprite_at_pixels(&mut self.hires, 0, col * 5, row * 11);
                }
                0 => {
                    if self.active[index] == Tile::Guard {
                        self.active[index] = Tile::Brick;
                        self.background[index] = Tile::Brick;
                        draw_sprite(&mut self.hires, 0, 1, col, row);
                        draw_sprite(&mut self.hires, 1, 1, col, row);
                        self.bury_guard(col, row, journal)?;
                        continue;
                    }
                    if self.active[index] == Tile::Player {
                        self.alive = false;
                        journal.push((
                            Subject::Actor(lode_trace::ActorId::Player),
                            lode_trace::Occurrence::Contact {
                                other: Subject::Cell(lode_trace::Cell { col, row }),
                            },
                        ));
                    }
                    if self.active[index] == Tile::Gold {
                        self.gold_count = self.gold_count.wrapping_sub(1);
                    }
                    self.active[index] = Tile::Brick;
                    draw_sprite(&mut self.hires, 0, 1, col, row);
                    draw_sprite(&mut self.hires, 1, 1, col, row);
                }
                _ => (),
            }
        }
        Ok(())
    }

    pub fn enable_exit_ladders(&mut self) {
        self.session.exit_available = true;
        let mut blocked = false;
        for slot in (0..self.hidden_ladders.len()).rev() {
            let cell = self.hidden_ladders[slot];
            let index = usize::from(cell.row) * 28 + usize::from(cell.col);
            if self.background[index] != Tile::Empty {
                blocked = true;
                continue;
            }
            self.background[index] = Tile::Ladder;
            if self.active[index] == Tile::Empty {
                self.active[index] = Tile::Ladder;
            }
            draw_sprite(&mut self.hires, 1, 3, cell.col, cell.row);
            draw_sprite_at_pixels(&mut self.hires, 3, cell.col * 5, cell.row * 11);
            self.hidden_ladders.remove(slot);
        }
        if !blocked {
            self.gold_count = 0xff;
            self.session.exit_available = true;
        }
    }
}

fn configuration(record: &[u8; 256], level: u8) -> String {
    let data: String = record.iter().map(|byte| format!("{byte:02x}")).collect();
    format!("guard-free-round-probe-v1:level={level}:{data}")
}

// The probe runs the implemented guard-free subset and refuses boards containing
// guards. It establishes the replay plumbing without claiming full game coverage.
pub fn capture_guard_free(
    record: &[u8; 256],
    level: u8,
    deliveries: &[Option<u8>],
) -> Result<Trace, String> {
    let mut game = Game::load_level(record, level, false)?;
    if game.guards.count != 0 {
        return Err("round probe requires a guard-free board".into());
    }
    let initial = game.snapshot();
    let header = Header {
        version: FORMAT_VERSION,
        source: "apple2-reference-113db1c".into(),
        configuration: configuration(record, level),
        initial: initial.clone(),
    };
    let mut observer = Observer::new(initial).map_err(|error| error.to_string())?;
    let mut rounds = Vec::new();
    let mut last_start = 0;
    for delivery in deliveries {
        if game.session.phase != Phase::Playing {
            return Err("input schedule extends past a terminal round".into());
        }
        last_start = game.clock.cycles;
        if let Some(code) = delivery {
            game.keyboard.deliver(*code);
        }
        let mut journal = Vec::new();
        let inputs = game.move_player(&mut journal)?;
        if game.alive {
            game.play_sound_section();
            if game.gold_count == 0 {
                game.enable_exit_ladders();
            }
            if game.player.row == 0
                && game.player.y_adjust == 2
                && matches!(game.gold_count, 0 | 0xff)
            {
                game.session.phase = Phase::LevelCleared;
            } else {
                game.handle_hole_timers(&mut journal)?;
                if game.alive {
                    game.play_sound_section();
                }
            }
        }
        if !game.alive {
            game.session.phase = Phase::Died;
        }
        rounds.push(
            observer
                .finish_round(last_start, game.snapshot(), inputs, journal)
                .map_err(|error| error.to_string())?,
        );
    }
    let trace = Trace {
        header,
        rounds,
        final_delta: game.clock.cycles - last_start,
        final_state: game.snapshot(),
    };
    trace.validate().map_err(|error| error.to_string())?;
    Ok(trace)
}

pub fn replay_guard_free(expected: &Trace, record: &[u8; 256], level: u8) -> Result<Trace, String> {
    expected.validate().map_err(|error| error.to_string())?;
    if expected.header.configuration != configuration(record, level) {
        return Err("trace configuration does not match replay board".into());
    }
    let mut deliveries = Vec::new();
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
        deliveries.push(round.inputs.first().map(|input| input.key));
    }
    let actual = capture_guard_free(record, level, &deliveries)?;
    if actual.header != expected.header {
        return Err("initial replay state or source differs".into());
    }
    for (expected, actual) in expected.rounds.iter().zip(&actual.rounds) {
        if expected != actual {
            return Err(format!(
                "round {}: replay differs from trace",
                expected.number
            ));
        }
    }
    if expected.final_state != actual.final_state || expected.final_delta != actual.final_delta {
        return Err("terminal replay state or timing differs".into());
    }
    Ok(actual)
}
