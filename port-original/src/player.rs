use crate::IMAGE;
use crate::graphics::{draw_sprite, draw_sprite_at_pixels, erase_sprite_at_pixels};
use crate::sound::{Note, play_note};
use crate::state::Game;
use lode_trace::{ActorId, Facing, Input, Motion, Occurrence, Subject, Tile};

pub type Journal = Vec<(Subject, Occurrence)>;

impl Game {
    pub fn player_should_fall(&self) -> bool {
        let row = usize::from(self.player.row);
        let col = usize::from(self.player.col);
        let here = self.background[row * 28 + col];
        if here == Tile::Ladder || (here == Tile::Rope && self.player.y_adjust == 2) {
            return false;
        }
        if self.player.y_adjust < 2 {
            return true;
        }
        if row == 15 {
            return false;
        }
        let below = (row + 1) * 28 + col;
        if self.active[below] == Tile::Empty {
            return true;
        }
        if self.active[below] == Tile::Guard {
            return false;
        }
        !matches!(
            self.background[below],
            Tile::Brick | Tile::Stone | Tile::Ladder
        )
    }

    // Gravity and continued digging bypass keyboard polling. A delivered key
    // remains in the device latch until a later movement operation observes it.
    pub fn move_player(&mut self, journal: &mut Journal) -> Result<Vec<Input>, String> {
        self.didnt_pick_up_gold = true;
        if self.dig_direction < 0 {
            self.continue_digging_left(journal)?;
            return Ok(vec![]);
        }
        if self.dig_direction > 0 {
            self.continue_digging_right(journal)?;
            return Ok(vec![]);
        }
        if self.player_should_fall() {
            self.fall_player(journal)?;
            return Ok(vec![]);
        }

        if self.falling_sound {
            let here =
                self.background[usize::from(self.player.row) * 28 + usize::from(self.player.col)];
            let low_counter = if here != Tile::Ladder
                && !(here == Tile::Rope && self.player.y_adjust == 2)
                && self.player.row == 15
                && self.player.y_adjust >= 2
            {
                self.player.row
            } else {
                self.player.col
            };
            play_note(
                &mut self.clock,
                &mut self.speaker,
                self.sound.enabled,
                0x64,
                8,
                low_counter,
            );
            journal.push((Subject::Actor(ActorId::Player), Occurrence::Landed));
        }
        self.falling_sound = false;
        self.falling_pitch = 0x20;
        self.player.motion = Motion::Standing;
        let code = self.keyboard.read_mmio(0xc000);
        self.keyboard.write_mmio(0xc010, code);
        let inputs = if code & 0x80 != 0 {
            if code < 0xa0 {
                return Err(format!("control key {code:02x} is not implemented yet"));
            }
            self.command = code;
            self.command_lr = code;
            vec![Input {
                observation: 0,
                key: code,
            }]
        } else {
            vec![]
        };

        let consumed = match self.command {
            0xc9 => self.try_moving_up(journal)?,
            0xcb => self.try_moving_down(journal)?,
            0xd5 => self.try_digging_left(journal)?,
            0xcf => self.try_digging_right(journal)?,
            _ => false,
        };
        if !consumed {
            match self.command_lr {
                0xca => {
                    self.try_moving_left(journal)?;
                }
                0xcc => {
                    self.try_moving_right(journal)?;
                }
                _ => (),
            }
        }
        Ok(inputs)
    }

    pub fn erase_player(&mut self) {
        let (sprite, x, y) = self.player_sprite_coordinates();
        erase_sprite_at_pixels(&mut self.hires, sprite, x, y);
    }

    pub fn draw_player(&mut self) {
        let (sprite, x, y) = self.player_sprite_coordinates();
        let overlap = draw_sprite_at_pixels(&mut self.hires, sprite, x, y);
        if overlap != 0 && self.didnt_pick_up_gold {
            self.alive = false;
        }
    }

    pub fn increment_player_animation(&mut self, lower: u8, upper: u8) {
        self.player.animation = self.player.animation.wrapping_add(1);
        if self.player.animation <= lower || self.player.animation > upper {
            self.player.animation = lower;
        }
    }

    pub fn add_and_update_score(&mut self, low: u8, high: u8) {
        self.score.add(low, high);
        self.session.score = self.score.decimal();
        let digits = [
            self.score.0[3] & 15,
            self.score.0[2] >> 4,
            self.score.0[2] & 15,
            self.score.0[1] >> 4,
            self.score.0[1] & 15,
            self.score.0[0] >> 4,
            self.score.0[0] & 15,
        ];
        for (index, digit) in digits.into_iter().enumerate() {
            draw_sprite(&mut self.hires, 0, 0x3b + digit, 5 + index as u8, 16);
        }
    }

    // Gold is collected only at both centered adjustments. The active map remains
    // occupied by the player; background and graphics lose the gold independently.
    pub fn check_player_gold(&mut self, journal: &mut Journal) -> Result<(), String> {
        if self.player.x_adjust != 2 || self.player.y_adjust != 2 {
            return Ok(());
        }
        let index = usize::from(self.player.row) * 28 + usize::from(self.player.col);
        if self.background[index] != Tile::Gold {
            return Ok(());
        }
        self.didnt_pick_up_gold = false;
        self.gold_count = self.gold_count.wrapping_sub(1);
        self.background[index] = Tile::Empty;
        draw_sprite(&mut self.hires, 1, 0, self.player.col, self.player.row);
        erase_sprite_at_pixels(
            &mut self.hires,
            7,
            self.player.col * 5,
            self.player.row * 11,
        );
        self.add_and_update_score(0x50, 0x02);
        self.sound.load_sound_data(&[
            Note {
                duration: 7,
                pitch: 0x45,
            },
            Note {
                duration: 6,
                pitch: 0x55,
            },
            Note {
                duration: 5,
                pitch: 0x44,
            },
            Note {
                duration: 4,
                pitch: 0x54,
            },
            Note {
                duration: 3,
                pitch: 0x43,
            },
            Note {
                duration: 2,
                pitch: 0x53,
            },
        ])?;
        journal.push((Subject::Actor(ActorId::Player), Occurrence::GoldCollected));
        Ok(())
    }

    pub fn nudge_player_column(&mut self, journal: &mut Journal) -> Result<(), String> {
        match self.player.x_adjust.cmp(&2) {
            std::cmp::Ordering::Less => self.player.x_adjust += 1,
            std::cmp::Ordering::Greater => self.player.x_adjust -= 1,
            std::cmp::Ordering::Equal => return Ok(()),
        }
        self.check_player_gold(journal)
    }

    pub fn nudge_player_row(&mut self, journal: &mut Journal) -> Result<(), String> {
        match self.player.y_adjust.cmp(&2) {
            std::cmp::Ordering::Less => self.player.y_adjust += 1,
            std::cmp::Ordering::Greater => self.player.y_adjust -= 1,
            std::cmp::Ordering::Equal => return Ok(()),
        }
        self.check_player_gold(journal)
    }

    pub fn fall_player(&mut self, journal: &mut Journal) -> Result<(), String> {
        self.falling_sound = true;
        self.erase_player();
        self.player.animation = if self.player.facing == Facing::Left {
            7
        } else {
            15
        };
        self.nudge_player_column(journal)?;
        self.player.y_adjust += 1;
        if self.player.y_adjust < 5 {
            self.check_player_gold(journal)?;
        } else {
            self.player.y_adjust = 0;
            let index = usize::from(self.player.row) * 28 + usize::from(self.player.col);
            self.active[index] = if self.background[index] == Tile::Brick {
                Tile::Empty
            } else {
                self.background[index]
            };
            self.player.row += 1;
            self.active[index + 28] = Tile::Player;
        }
        self.player.motion = Motion::Falling;
        self.draw_player();
        Ok(())
    }

    pub fn try_moving_left(&mut self, journal: &mut Journal) -> Result<bool, String> {
        let index = usize::from(self.player.row) * 28 + usize::from(self.player.col);
        if self.player.x_adjust < 3
            && (self.player.col == 0
                || matches!(
                    self.active[index - 1],
                    Tile::Brick | Tile::Stone | Tile::Trap
                ))
        {
            return Ok(false);
        }
        self.erase_player();
        self.player.facing = Facing::Left;
        self.nudge_player_row(journal)?;
        self.player.x_adjust = self.player.x_adjust.wrapping_sub(1);
        if self.player.x_adjust & 0x80 != 0 {
            self.active[index] = if self.background[index] == Tile::Brick {
                Tile::Empty
            } else {
                self.background[index]
            };
            self.player.col -= 1;
            self.active[index - 1] = Tile::Player;
            self.player.x_adjust = 4;
        } else {
            self.check_player_gold(journal)?;
        }
        let rope = self.background
            [usize::from(self.player.row) * 28 + usize::from(self.player.col)]
            == Tile::Rope;
        self.increment_player_animation(if rope { 3 } else { 0 }, if rope { 5 } else { 2 });
        self.player.motion = Motion::Left;
        self.draw_player();
        Ok(true)
    }

    pub fn try_moving_right(&mut self, journal: &mut Journal) -> Result<bool, String> {
        let index = usize::from(self.player.row) * 28 + usize::from(self.player.col);
        if self.player.x_adjust >= 2
            && (self.player.col == 27
                || matches!(
                    self.active[index + 1],
                    Tile::Brick | Tile::Stone | Tile::Trap
                ))
        {
            return Ok(false);
        }
        self.erase_player();
        self.player.facing = Facing::Right;
        self.nudge_player_row(journal)?;
        self.player.x_adjust += 1;
        if self.player.x_adjust >= 5 {
            self.active[index] = if self.background[index] == Tile::Brick {
                Tile::Empty
            } else {
                self.background[index]
            };
            self.player.col += 1;
            self.active[index + 1] = Tile::Player;
            self.player.x_adjust = 0;
        } else {
            self.check_player_gold(journal)?;
        }
        let rope = self.background
            [usize::from(self.player.row) * 28 + usize::from(self.player.col)]
            == Tile::Rope;
        self.increment_player_animation(if rope { 11 } else { 8 }, if rope { 13 } else { 10 });
        self.player.motion = Motion::Right;
        self.draw_player();
        Ok(true)
    }

    pub fn try_moving_up(&mut self, journal: &mut Journal) -> Result<bool, String> {
        let index = usize::from(self.player.row) * 28 + usize::from(self.player.col);
        if self.background[index] != Tile::Ladder {
            if self.player.y_adjust <= 2
                || self.player.row == 15
                || self.background[index + 28] != Tile::Ladder
            {
                return Ok(false);
            }
        } else if self.player.y_adjust <= 2
            && (self.player.row == 0
                || matches!(
                    self.active[index - 28],
                    Tile::Brick | Tile::Stone | Tile::Trap
                ))
        {
            return Ok(false);
        }
        self.erase_player();
        self.nudge_player_column(journal)?;
        self.player.y_adjust = self.player.y_adjust.wrapping_sub(1);
        if self.player.y_adjust & 0x80 != 0 {
            self.active[index] = if self.background[index] == Tile::Brick {
                Tile::Empty
            } else {
                self.background[index]
            };
            self.player.row -= 1;
            self.active[index - 28] = Tile::Player;
            self.player.y_adjust = 4;
        } else {
            self.check_player_gold(journal)?;
        }
        self.increment_player_animation(16, 17);
        self.player.motion = Motion::Up;
        self.draw_player();
        Ok(true)
    }

    pub fn try_moving_down(&mut self, journal: &mut Journal) -> Result<bool, String> {
        let index = usize::from(self.player.row) * 28 + usize::from(self.player.col);
        if self.player.y_adjust >= 2
            && (self.player.row == 15
                || matches!(self.active[index + 28], Tile::Brick | Tile::Stone))
        {
            return Ok(false);
        }
        self.erase_player();
        self.nudge_player_column(journal)?;
        self.player.y_adjust += 1;
        if self.player.y_adjust >= 5 {
            self.active[index] = if self.background[index] == Tile::Brick {
                Tile::Empty
            } else {
                self.background[index]
            };
            self.player.row += 1;
            self.active[index + 28] = Tile::Player;
            self.player.y_adjust = 0;
        } else {
            self.check_player_gold(journal)?;
        }
        self.increment_player_animation(16, 17);
        self.player.motion = Motion::Down;
        self.draw_player();
        Ok(true)
    }

    // Digging uses twelve visual stages followed by a thirteenth completion call.
    // Only the active logical brick is removed; the background terrain stays brick.
    pub fn try_digging_left(&mut self, journal: &mut Journal) -> Result<bool, String> {
        self.dig_direction = -1;
        self.command = 0xff;
        self.command_lr = 0xff;
        self.dig_animation = 0;
        self.continue_digging_left(journal)
    }

    pub fn continue_digging_left(&mut self, journal: &mut Journal) -> Result<bool, String> {
        if self.player.row == 15 || self.player.col == 0 {
            self.dig_direction = 0;
            return Ok(false);
        }
        let index = usize::from(self.player.row) * 28 + usize::from(self.player.col) - 1;
        if self.active[index + 28] != Tile::Brick {
            self.dig_direction = 0;
            return Ok(false);
        }
        if self.active[index] != Tile::Empty {
            draw_sprite(
                &mut self.hires,
                0,
                1,
                self.player.col - 1,
                self.player.row + 1,
            );
            if self.dig_animation != 0 {
                erase_sprite_at_pixels(
                    &mut self.hires,
                    IMAGE[0x697a + usize::from(self.dig_animation) - 1],
                    (self.player.col - 1) * 5,
                    self.player.row * 11,
                );
            }
            self.dig_direction = 0;
            return Ok(false);
        }
        self.erase_player();
        self.nudge_player_column(journal)?;
        self.nudge_player_row(journal)?;
        self.sound.append_note(Note {
            duration: IMAGE[0x69ab + usize::from(self.dig_animation)],
            pitch: IMAGE[0x699e + usize::from(self.dig_animation)],
        })?;
        self.player.animation = if self.dig_animation < 6 { 6 } else { 0 };
        self.draw_player();
        if self.dig_animation == 12 {
            self.open_hole(self.player.col - 1);
            return Ok(false);
        }
        if self.dig_animation != 0 {
            erase_sprite_at_pixels(
                &mut self.hires,
                IMAGE[0x697a + usize::from(self.dig_animation) - 1],
                (self.player.col - 1) * 5,
                self.player.row * 11,
            );
        }
        draw_sprite_at_pixels(
            &mut self.hires,
            IMAGE[0x697a + usize::from(self.dig_animation)],
            (self.player.col - 1) * 5,
            self.player.row * 11,
        );
        draw_sprite(
            &mut self.hires,
            0,
            IMAGE[0x6992 + usize::from(self.dig_animation)],
            self.player.col - 1,
            self.player.row + 1,
        );
        self.dig_animation += 1;
        Ok(true)
    }

    pub fn try_digging_right(&mut self, journal: &mut Journal) -> Result<bool, String> {
        self.dig_direction = 1;
        self.command = 1;
        self.command_lr = 1;
        self.dig_animation = 12;
        self.continue_digging_right(journal)
    }

    pub fn continue_digging_right(&mut self, journal: &mut Journal) -> Result<bool, String> {
        if self.player.row == 15 || self.player.col == 27 {
            self.dig_direction = 0;
            return Ok(false);
        }
        let index = usize::from(self.player.row) * 28 + usize::from(self.player.col) + 1;
        if self.active[index + 28] != Tile::Brick {
            self.dig_direction = 0;
            return Ok(false);
        }
        if self.active[index] != Tile::Empty {
            draw_sprite(
                &mut self.hires,
                0,
                1,
                self.player.col + 1,
                self.player.row + 1,
            );
            if self.dig_animation != 12 {
                erase_sprite_at_pixels(
                    &mut self.hires,
                    IMAGE[0x697a + usize::from(self.dig_animation) - 1],
                    (self.player.col + 1) * 5,
                    self.player.row * 11,
                );
            }
            self.dig_direction = 0;
            return Ok(false);
        }
        self.erase_player();
        self.nudge_player_column(journal)?;
        self.nudge_player_row(journal)?;
        let stage = usize::from(self.dig_animation - 12);
        self.sound.append_note(Note {
            duration: IMAGE[0x69ab + stage],
            pitch: IMAGE[0x699e + stage],
        })?;
        self.player.animation = if self.dig_animation < 18 { 14 } else { 8 };
        self.draw_player();
        if self.dig_animation == 24 {
            self.open_hole(self.player.col + 1);
            return Ok(false);
        }
        if self.dig_animation != 12 {
            erase_sprite_at_pixels(
                &mut self.hires,
                IMAGE[0x697a + usize::from(self.dig_animation) - 1],
                (self.player.col + 1) * 5,
                self.player.row * 11,
            );
        }
        draw_sprite_at_pixels(
            &mut self.hires,
            IMAGE[0x697a + usize::from(self.dig_animation)],
            (self.player.col + 1) * 5,
            self.player.row * 11,
        );
        draw_sprite(
            &mut self.hires,
            0,
            IMAGE[0x6992 + stage],
            self.player.col + 1,
            self.player.row + 1,
        );
        self.dig_animation += 1;
        Ok(true)
    }

    pub fn open_hole(&mut self, col: u8) {
        self.dig_direction = 0;
        let row = self.player.row + 1;
        self.active[usize::from(row) * 28 + usize::from(col)] = Tile::Empty;
        draw_sprite(&mut self.hires, 0, 0, col, row);
        draw_sprite(&mut self.hires, 1, 0, col, row);
        draw_sprite(&mut self.hires, 0, 0, col, row - 1);
        for slot in 0..30 {
            if self.holes.timers[slot] == 0 {
                self.holes.cols[slot] = col;
                self.holes.rows[slot] = row;
                self.holes.timers[slot] = 180;
                let index = usize::from(row) * 28 + usize::from(col);
                self.hole_generations[index] += 1;
                self.holes.generations[slot] = self.hole_generations[index];
                break;
            }
        }
    }
}
