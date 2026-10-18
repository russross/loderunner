use crate::board::{Board, GoldProgress};
use crate::collision::Layer;
use crate::event_timing::{SoundSection, TimingEvent};
use crate::graphics::Sprite;
use crate::player::Journal;
use crate::sound::play_note;
use crate::state::Game;
use lode_trace::{ActorId, Input, Occurrence, Phase, Subject, Tile};
use std::num::NonZeroU8;

impl Game {
    // A gameplay round updates the player first. Sound and exit checks precede
    // timers and guards; terminal paths stop at their original gameplay boundary.
    pub fn play_round(&mut self, journal: &mut Journal) -> Result<Vec<Input>, String> {
        let inputs = self.update_player(journal)?;
        if !self.player.alive {
            self.session.phase = Phase::Died;
            return Ok(inputs);
        }
        if self.session.phase != Phase::Playing {
            return Ok(inputs);
        }
        self.play_sound_section();
        if self.gold == GoldProgress::RevealPending {
            self.enable_exit_ladders();
        }
        let position = self.player.position;
        if position.cell.row == 0 && position.y() == 2 && self.gold.allows_completion() {
            self.session.phase = Phase::LevelCleared;
            return Ok(inputs);
        }
        self.update_resurrections()?;
        self.resurrection_col = (self.resurrection_col + 1) % 28;
        self.update_holes(journal)?;
        if self.player.alive {
            self.play_sound_section();
            self.update_guards(journal)?;
        }
        if !self.player.alive {
            self.session.phase = Phase::Died;
        }
        Ok(inputs)
    }

    pub fn play_sound_section(&mut self) {
        self.clock.flush_draws();
        let section = match self.sound.peek() {
            Some(note) => SoundSection::Buffered {
                duration: note.duration,
                period: self.sound.frame_period,
            },
            None if self.falling_sound => SoundSection::Falling,
            None => SoundSection::Idle {
                period: self.sound.frame_period,
            },
        };
        self.clock.events.begin_sound(section);
        match section {
            SoundSection::Buffered { .. } => {
                let low_counter = self.sound.len() as u8;
                let note = self.sound.pop().expect("buffered note");
                play_note(
                    &mut self.clock,
                    self.sound.enabled,
                    note.pitch,
                    note.duration,
                    low_counter,
                );
            }
            SoundSection::Falling => {
                let pitch = self.falling_pitch >> 1;
                self.falling_pitch = self.falling_pitch.wrapping_add(1);
                play_note(
                    &mut self.clock,
                    self.sound.enabled,
                    pitch,
                    self.sound.frame_period + 2,
                    0,
                );
            }
            SoundSection::Idle { .. } => (),
        }
        self.clock.events.end_sound(section);
    }

    // Refilling preserves brick terrain while restoring occupancy and both scene
    // layers. Reverse allocation order determines concurrent deaths and respawns.
    pub fn update_holes(&mut self, journal: &mut Journal) -> Result<(), String> {
        self.clock.events.add(TimingEvent::HoleScan, 1);
        for slot in (0..self.holes.entries.len()).rev() {
            let Some(mut hole) = self.holes.entries[slot] else {
                continue;
            };
            self.clock.events.add(TimingEvent::ActiveHole, 1);
            let remaining = hole.remaining.get() - 1;
            if let Some(timer) = NonZeroU8::new(remaining) {
                hole.remaining = timer;
                self.holes.entries[slot] = Some(hole);
                if matches!(remaining, 20 | 10) {
                    self.paint_cell(
                        Layer::Background,
                        if remaining == 20 {
                            Sprite::REFILL_FIRST
                        } else {
                            Sprite::REFILL_SECOND
                        },
                        hole.cell,
                    );
                    self.erase_cell(Sprite::EMPTY, hole.cell);
                }
                continue;
            }
            self.holes.entries[slot] = None;
            match self.board.at(hole.cell) {
                Tile::Guard => {
                    self.board.occupy(hole.cell, Tile::Brick);
                    self.board.terrain[Board::index(hole.cell)] = Tile::Brick;
                    self.paint_cell(Layer::Foreground, Sprite::BRICK, hole.cell);
                    self.paint_cell(Layer::Background, Sprite::BRICK, hole.cell);
                    self.bury_guard(hole.cell, journal)?;
                    continue;
                }
                Tile::Player => {
                    self.player.alive = false;
                    journal.push((
                        Subject::Actor(ActorId::Player),
                        Occurrence::Contact {
                            other: Subject::Cell(hole.cell),
                        },
                    ));
                }
                Tile::Gold => self.gold.remove_one(),
                _ => (),
            }
            self.board.occupy(hole.cell, Tile::Brick);
            self.paint_cell(Layer::Foreground, Sprite::BRICK, hole.cell);
            self.paint_cell(Layer::Background, Sprite::BRICK, hole.cell);
        }
        Ok(())
    }

    pub fn enable_exit_ladders(&mut self) {
        self.session.exit_available = true;
        let mut blocked = false;
        for slot in (0..self.hidden_ladders.len()).rev() {
            let cell = self.hidden_ladders[slot];
            if self.board.terrain_at(cell) != Tile::Empty {
                blocked = true;
                continue;
            }
            self.board.terrain[usize::from(cell.row) * 28 + usize::from(cell.col)] = Tile::Ladder;
            if self.board.at(cell) == Tile::Empty {
                self.board.occupy(cell, Tile::Ladder);
            }
            self.paint_cell(Layer::Background, Sprite::LADDER, cell);
            self.overlay_cell(Sprite::LADDER, cell);
            self.hidden_ladders.remove(slot);
        }
        if !blocked {
            self.gold = GoldProgress::Revealed;
        }
    }
}
