use crate::IMAGE;
use crate::graphics::{draw_sprite, draw_sprite_at_pixels, erase_sprite_at_pixels};
use crate::player::Journal;
use crate::sound::Note;
use crate::state::Game;
use lode_trace::{ActorId, Cell, Motion, Occurrence, Subject, Tile};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuardAction {
    Stay,
    Left,
    Right,
    Up,
    Down,
}

#[derive(Clone, Copy)]
pub struct Guard {
    pub slot: usize,
    pub col: u8,
    pub row: u8,
    pub x: u8,
    pub y: u8,
    pub animation: u8,
    pub facing: u8,
    pub gold: u8,
    pub motion: Motion,
}

impl Game {
    pub fn load_guard(&self, slot: usize) -> Guard {
        Guard {
            slot,
            col: self.guards.cols[slot],
            row: self.guards.rows[slot],
            x: self.guards.x_adjusts[slot],
            y: self.guards.y_adjusts[slot],
            animation: self.guards.animations[slot],
            facing: self.guards.facings[slot],
            gold: self.guards.gold_timers[slot],
            motion: self.guards.motions[slot].unwrap_or(Motion::Standing),
        }
    }

    pub fn store_guard(&mut self, guard: Guard) {
        let slot = guard.slot;
        self.guards.cols[slot] = guard.col;
        self.guards.rows[slot] = guard.row;
        self.guards.x_adjusts[slot] = guard.x;
        self.guards.y_adjusts[slot] = guard.y;
        self.guards.animations[slot] = guard.animation;
        self.guards.facings[slot] = guard.facing;
        self.guards.gold_timers[slot] = guard.gold;
        self.guards.motions[slot] = Some(guard.motion);
    }

    pub fn guard_coordinates(&self, guard: Guard) -> (u8, u8, u8) {
        let sprite = IMAGE[0x6ccb + usize::from(guard.animation)];
        let x = (guard.col * 5).wrapping_add(guard.x).wrapping_sub(2);
        let y = (guard.row * 11).wrapping_add(IMAGE[0x888a + usize::from(guard.y)]);
        (sprite, x, y)
    }

    pub fn erase_guard(&mut self, guard: Guard) {
        let (sprite, x, y) = self.guard_coordinates(guard);
        erase_sprite_at_pixels(&mut self.hires, sprite, x, y);
    }

    pub fn draw_guard(&mut self, guard: Guard) {
        let (sprite, x, y) = self.guard_coordinates(guard);
        draw_sprite_at_pixels(&mut self.hires, sprite, x, y);
    }

    pub fn move_guards(&mut self, journal: &mut Journal) -> Result<(), String> {
        if self.guards.count == 0 {
            return Ok(());
        }
        self.guards.phase = (self.guards.phase + 1) % 3;
        let pattern_index = usize::from(self.guards.count) + usize::from(self.difficulty);
        if pattern_index > 11 {
            return Err("guard difficulty exceeds original pattern table".into());
        }
        let mut pattern = IMAGE[0x6ca7 + pattern_index * 3 + usize::from(self.guards.phase)];
        while pattern != 0 {
            let move_next = pattern & 1 != 0;
            pattern >>= 1;
            if move_next {
                self.guards.current += 1;
                if self.guards.current > self.guards.count {
                    self.guards.current = 1;
                }
                self.move_guard(usize::from(self.guards.current), journal)?;
                if !self.alive {
                    break;
                }
            }
        }
        Ok(())
    }

    // Positive gold counters also encode the trapped animation and escape delay.
    // Negative counters encode carrying gold; zero is an ordinary active guard.
    pub fn move_guard(&mut self, slot: usize, journal: &mut Journal) -> Result<(), String> {
        self.clock.advance(200);
        let mut guard = self.load_guard(slot);
        guard.motion = Motion::Standing;
        if guard.gold != 0 && guard.gold < 0x80 {
            guard.gold -= 1;
            if guard.gold >= 13 {
                if self.guards.resurrection_timers[slot] == 0 {
                    self.draw_guard(guard);
                }
                self.store_guard(guard);
                return Ok(());
            }
            if guard.gold >= 7 {
                self.erase_guard(guard);
                guard.x = IMAGE[0x6e7f + usize::from(guard.gold - 7)];
                self.draw_guard(guard);
                self.store_guard(guard);
                return Ok(());
            }
        } else if self.guard_should_fall(guard) {
            self.fall_guard(&mut guard, journal)?;
            self.store_guard(guard);
            return Ok(());
        }
        let action = self.determine_guard_move(guard);
        self.try_guard_move(&mut guard, action, journal)?;
        self.store_guard(guard);
        Ok(())
    }

    fn guard_should_fall(&self, guard: Guard) -> bool {
        let index = usize::from(guard.row) * 28 + usize::from(guard.col);
        if self.background[index] == Tile::Ladder
            || (self.background[index] == Tile::Rope && guard.y == 2)
        {
            return false;
        }
        if guard.y < 2 {
            return true;
        }
        if guard.row == 15 {
            return false;
        }
        if matches!(self.active[index + 28], Tile::Empty | Tile::Player) {
            return true;
        }
        if self.active[index + 28] == Tile::Guard {
            return false;
        }
        !matches!(
            self.background[index + 28],
            Tile::Brick | Tile::Stone | Tile::Ladder
        )
    }

    pub fn check_guard_gold(&mut self, guard: &mut Guard, journal: &mut Journal) {
        if guard.x != 2 || guard.y != 2 || guard.gold & 0x80 != 0 {
            return;
        }
        let index = usize::from(guard.row) * 28 + usize::from(guard.col);
        if self.background[index] != Tile::Gold {
            return;
        }
        guard.gold = 0xff - self.resurrection_col;
        self.background[index] = Tile::Empty;
        draw_sprite(&mut self.hires, 1, 0, guard.col, guard.row);
        erase_sprite_at_pixels(&mut self.hires, 7, guard.col * 5, guard.row * 11);
        journal.push((
            Subject::Actor(ActorId::Guard(guard.slot as u8)),
            Occurrence::GoldCollected,
        ));
    }

    pub fn guard_drop_gold(&mut self, guard: &mut Guard, journal: &mut Journal) {
        if guard.gold & 0x80 == 0 {
            return;
        }
        guard.gold = guard.gold.wrapping_add(1);
        if guard.gold != 0 {
            return;
        }
        let index = usize::from(guard.row) * 28 + usize::from(guard.col);
        if self.background[index] != Tile::Empty {
            guard.gold = 0xff;
            return;
        }
        self.background[index] = Tile::Gold;
        draw_sprite(&mut self.hires, 1, 7, guard.col, guard.row);
        draw_sprite_at_pixels(&mut self.hires, 7, guard.col * 5, guard.row * 11);
        journal.push((
            Subject::Actor(ActorId::Guard(guard.slot as u8)),
            Occurrence::GoldDropped {
                cell: Cell {
                    col: guard.col,
                    row: guard.row,
                },
            },
        ));
    }

    fn nudge_guard_column(&mut self, guard: &mut Guard, journal: &mut Journal) {
        if guard.x == 2 {
            return;
        }
        if guard.x < 2 {
            guard.x += 1;
        } else {
            guard.x -= 1;
        }
        self.check_guard_gold(guard, journal);
    }

    fn nudge_guard_row(&mut self, guard: &mut Guard, journal: &mut Journal) {
        if guard.y == 2 {
            return;
        }
        if guard.y < 2 {
            guard.y += 1;
        } else {
            guard.y -= 1;
        }
        self.check_guard_gold(guard, journal);
    }

    fn occupy_guard_cell(&mut self, guard: Guard, journal: &mut Journal) {
        let index = usize::from(guard.row) * 28 + usize::from(guard.col);
        if self.active[index] == Tile::Player {
            self.alive = false;
            journal.push((
                Subject::Actor(ActorId::Player),
                Occurrence::Contact {
                    other: Subject::Actor(ActorId::Guard(guard.slot as u8)),
                },
            ));
        }
        self.active[index] = Tile::Guard;
    }

    fn fall_guard(&mut self, guard: &mut Guard, journal: &mut Journal) -> Result<(), String> {
        self.erase_guard(*guard);
        self.nudge_guard_column(guard, journal);
        guard.animation = if guard.facing & 0x80 != 0 { 6 } else { 13 };
        guard.y += 1;
        guard.motion = Motion::Falling;
        if guard.y < 5 {
            if guard.y == 2 {
                self.check_guard_gold(guard, journal);
                let index = usize::from(guard.row) * 28 + usize::from(guard.col);
                if self.background[index] == Tile::Brick {
                    if guard.gold & 0x80 != 0 {
                        self.gold_count = self.gold_count.wrapping_sub(1);
                    }
                    guard.gold = IMAGE[0x621d + usize::from(self.difficulty)];
                    self.add_and_update_score(0x75, 0);
                    self.sound.load_sound_data(&[
                        Note {
                            duration: 6,
                            pitch: 0x20,
                        },
                        Note {
                            duration: 4,
                            pitch: 0x30,
                        },
                        Note {
                            duration: 2,
                            pitch: 0x40,
                        },
                    ])?;
                    journal.push((
                        Subject::Actor(ActorId::Guard(guard.slot as u8)),
                        Occurrence::Trapped,
                    ));
                }
            }
        } else {
            guard.y = 0;
            let index = usize::from(guard.row) * 28 + usize::from(guard.col);
            self.active[index] = if self.background[index] == Tile::Brick {
                Tile::Empty
            } else {
                self.background[index]
            };
            guard.row += 1;
            let next = index + 28;
            if self.active[next] == Tile::Player {
                self.alive = false;
                journal.push((
                    Subject::Actor(ActorId::Player),
                    Occurrence::Contact {
                        other: Subject::Actor(ActorId::Guard(guard.slot as u8)),
                    },
                ));
            }
            if self.background[next] == Tile::Brick && guard.gold & 0x80 != 0 {
                if self.background[index] == Tile::Empty {
                    self.background[index] = Tile::Gold;
                    self.active[index] = Tile::Gold;
                    draw_sprite(&mut self.hires, 1, 7, guard.col, guard.row - 1);
                    draw_sprite_at_pixels(&mut self.hires, 7, guard.col * 5, (guard.row - 1) * 11);
                    journal.push((
                        Subject::Actor(ActorId::Guard(guard.slot as u8)),
                        Occurrence::GoldDropped {
                            cell: Cell {
                                col: guard.col,
                                row: guard.row - 1,
                            },
                        },
                    ));
                } else {
                    self.gold_count = self.gold_count.wrapping_sub(1);
                }
                guard.gold = 0;
            }
            self.active[next] = Tile::Guard;
        }
        self.draw_guard(*guard);
        Ok(())
    }

    // Movement eligibility differs by direction. A blocked upward escape restores
    // the positive timer increment; other blocked paths simply store working state.
    pub fn try_guard_move(
        &mut self,
        guard: &mut Guard,
        action: GuardAction,
        journal: &mut Journal,
    ) -> Result<(), String> {
        self.clock.advance(180);
        let index = usize::from(guard.row) * 28 + usize::from(guard.col);
        let blocked = match action {
            GuardAction::Stay => true,
            GuardAction::Left => {
                guard.x < 3
                    && (guard.col == 0
                        || matches!(
                            self.active[index - 1],
                            Tile::Guard | Tile::Stone | Tile::Brick
                        )
                        || self.background[index - 1] == Tile::Trap)
            }
            GuardAction::Right => {
                guard.x >= 2
                    && (guard.col == 27
                        || matches!(
                            self.active[index + 1],
                            Tile::Guard | Tile::Stone | Tile::Brick
                        )
                        || self.background[index + 1] == Tile::Trap)
            }
            GuardAction::Up => {
                guard.y < 3
                    && (guard.row == 0
                        || matches!(
                            self.active[index - 28],
                            Tile::Brick | Tile::Stone | Tile::Trap | Tile::Guard
                        ))
            }
            GuardAction::Down => {
                guard.y >= 2
                    && (guard.row == 15
                        || matches!(
                            self.active[index + 28],
                            Tile::Brick | Tile::Stone | Tile::Guard
                        ))
            }
        };
        if blocked {
            if action == GuardAction::Up && (1..0x80).contains(&guard.gold) {
                guard.gold += 1;
            }
            return Ok(());
        }
        self.erase_guard(*guard);
        match action {
            GuardAction::Left | GuardAction::Right => self.nudge_guard_row(guard, journal),
            _ => self.nudge_guard_column(guard, journal),
        }
        match action {
            GuardAction::Left => {
                guard.facing = 0xff;
                guard.x = guard.x.wrapping_sub(1);
                guard.motion = Motion::Left;
            }
            GuardAction::Right => {
                guard.facing = 1;
                guard.x += 1;
                guard.motion = Motion::Right;
            }
            GuardAction::Up => {
                guard.y = guard.y.wrapping_sub(1);
                guard.motion = Motion::Up;
            }
            GuardAction::Down => {
                guard.y += 1;
                guard.motion = Motion::Down;
            }
            GuardAction::Stay => unreachable!(),
        }
        if guard.x == 0xff || guard.x == 5 || guard.y == 0xff || guard.y == 5 {
            self.guard_drop_gold(guard, journal);
            self.active[index] = if self.background[index] == Tile::Brick {
                Tile::Empty
            } else {
                self.background[index]
            };
            match action {
                GuardAction::Left => {
                    guard.col -= 1;
                    guard.x = 4;
                }
                GuardAction::Right => {
                    guard.col += 1;
                    guard.x = 0;
                }
                GuardAction::Up => {
                    guard.row -= 1;
                    guard.y = 4;
                }
                GuardAction::Down => {
                    guard.row += 1;
                    guard.y = 0;
                }
                _ => unreachable!(),
            }
            self.occupy_guard_cell(*guard, journal);
        } else {
            self.check_guard_gold(guard, journal);
        }
        let rope =
            self.background[usize::from(guard.row) * 28 + usize::from(guard.col)] == Tile::Rope;
        let (lower, upper) = match action {
            GuardAction::Left => {
                if rope {
                    (3, 5)
                } else {
                    (0, 2)
                }
            }
            GuardAction::Right => {
                if rope {
                    (10, 12)
                } else {
                    (7, 9)
                }
            }
            _ => (14, 15),
        };
        guard.animation = guard.animation.wrapping_add(1);
        if guard.animation <= lower || guard.animation > upper {
            guard.animation = lower;
        }
        self.draw_guard(*guard);
        Ok(())
    }

    pub fn determine_guard_move(&mut self, guard: Guard) -> GuardAction {
        let index = usize::from(guard.row) * 28 + usize::from(guard.col);
        if self.background[index] == Tile::Brick && (1..0x80).contains(&guard.gold) {
            return GuardAction::Up;
        }
        if guard.row == self.player.row && guard.col != self.player.col {
            let direction = if guard.col < self.player.col {
                1_i16
            } else {
                -1
            };
            let mut col = i16::from(guard.col);
            let mut clear = true;
            while col != i16::from(self.player.col) {
                col += direction;
                let index = usize::from(guard.row) * 28 + col as usize;
                if !matches!(self.background[index], Tile::Ladder | Tile::Rope)
                    && guard.row != 15
                    && matches!(self.background[index + 28], Tile::Empty | Tile::Trap)
                {
                    clear = false;
                    break;
                }
            }
            if clear {
                return if direction > 0 {
                    GuardAction::Right
                } else {
                    GuardAction::Left
                };
            }
        }
        let (left, right) = self.guard_horizontal_limits(guard);
        self.clock
            .advance(75 * u64::from(guard.col - left + right - guard.col));
        let mut best = 255;
        let mut action = GuardAction::Stay;
        self.consider_guard_routes(
            guard,
            guard.col,
            GuardAction::Down,
            GuardAction::Up,
            &mut best,
            &mut action,
        );
        for col in left..guard.col {
            self.consider_guard_routes(
                guard,
                col,
                GuardAction::Left,
                GuardAction::Left,
                &mut best,
                &mut action,
            );
        }
        for col in (guard.col + 1..=right).rev() {
            self.consider_guard_routes(
                guard,
                col,
                GuardAction::Right,
                GuardAction::Right,
                &mut best,
                &mut action,
            );
        }
        action
    }

    fn consider_guard_routes(
        &mut self,
        guard: Guard,
        col: u8,
        down: GuardAction,
        up: GuardAction,
        best: &mut u8,
        action: &mut GuardAction,
    ) {
        self.clock.advance(100);
        let index = usize::from(guard.row) * 28 + usize::from(col);
        if guard.row < 15 && !matches!(self.background[index + 28], Tile::Brick | Tile::Stone) {
            let row = self.guard_candidate_below(col, guard.row);
            let distance = self.guard_distance(guard, col, row);
            if distance < *best {
                *best = distance;
                *action = down;
            }
        }
        if guard.row > 0 && self.background[index] == Tile::Ladder {
            let row = self.guard_candidate_above(col, guard.row);
            let distance = self.guard_distance(guard, col, row);
            if distance < *best {
                *best = distance;
                *action = up;
            }
        }
    }

    fn guard_distance(&self, guard: Guard, col: u8, row: u8) -> u8 {
        if row == self.player.row {
            col.abs_diff(guard.col)
        } else if row > self.player.row {
            200 + row - self.player.row
        } else {
            100 + self.player.row - row
        }
    }

    fn guard_horizontal_limits(&self, guard: Guard) -> (u8, u8) {
        let traversable = |col: u8| {
            let index = usize::from(guard.row) * 28 + usize::from(col);
            if matches!(self.active[index], Tile::Brick | Tile::Stone) {
                return (false, false);
            }
            (
                true,
                matches!(self.active[index], Tile::Ladder | Tile::Rope)
                    || guard.row == 15
                    || matches!(
                        self.background[index + 28],
                        Tile::Brick | Tile::Stone | Tile::Ladder
                    ),
            )
        };
        let mut left = guard.col;
        while left > 0 {
            let (can_enter, can_continue) = traversable(left - 1);
            if !can_enter {
                break;
            }
            left -= 1;
            if !can_continue {
                break;
            }
        }
        let mut right = guard.col;
        while right < 27 {
            let (can_enter, can_continue) = traversable(right + 1);
            if !can_enter {
                break;
            }
            right += 1;
            if !can_continue {
                break;
            }
        }
        (left, right)
    }

    fn guard_candidate_above(&mut self, col: u8, mut row: u8) -> u8 {
        loop {
            self.clock.advance(100);
            let old = usize::from(row) * 28 + usize::from(col);
            if self.background[old] != Tile::Ladder {
                return row;
            }
            row = row.wrapping_sub(1);
            if row == 0xff {
                return 0xff;
            }
            let index = usize::from(row) * 28 + usize::from(col);
            if col > 0
                && (matches!(
                    self.background[old - 1],
                    Tile::Brick | Tile::Stone | Tile::Ladder
                ) || self.background[index - 1] == Tile::Rope)
                && row <= self.player.row
            {
                return row;
            }
            if col < 27
                && (matches!(
                    self.background[old + 1],
                    Tile::Brick | Tile::Stone | Tile::Ladder
                ) || self.background[index + 1] == Tile::Rope)
                && row <= self.player.row
            {
                return row;
            }
            if row == 0 {
                return row;
            }
        }
    }

    fn guard_candidate_below(&mut self, col: u8, mut row: u8) -> u8 {
        loop {
            self.clock.advance(100);
            if row == 15 {
                return 15;
            }
            let index = usize::from(row) * 28 + usize::from(col);
            if matches!(self.background[index + 28], Tile::Brick | Tile::Stone) {
                return row;
            }
            let mut pointer_row = row;
            if self.background[index] != Tile::Empty {
                if col > 0 {
                    if self.background[index - 1] == Tile::Rope && row >= self.player.row {
                        return row;
                    }
                    if self.background[index - 1] != Tile::Rope {
                        pointer_row = row + 1;
                        if matches!(
                            self.background[index + 27],
                            Tile::Brick | Tile::Stone | Tile::Ladder
                        ) && row >= self.player.row
                        {
                            return row;
                        }
                    }
                }
                if col < 27 {
                    // The original right check retains whichever row pointer the
                    // left check left behind. This asymmetry affects route choice.
                    let pointer = usize::from(pointer_row) * 28 + usize::from(col);
                    if self.background[pointer + 1] == Tile::Rope && row >= self.player.row {
                        return row;
                    }
                    if self.background[pointer + 1] != Tile::Rope
                        && matches!(
                            self.background[index + 29],
                            Tile::Brick | Tile::Stone | Tile::Ladder
                        )
                        && row >= self.player.row
                    {
                        return row;
                    }
                }
            }
            row += 1;
        }
    }

    pub fn bury_guard(&mut self, col: u8, row: u8, journal: &mut Journal) -> Result<(), String> {
        let slot = (1..=usize::from(self.guards.count))
            .rev()
            .find(|&slot| self.guards.cols[slot] == col && self.guards.rows[slot] == row)
            .ok_or("guard occupancy has no matching guard")?;
        let mut guard = self.load_guard(slot);
        if guard.gold & 0x80 != 0 {
            self.gold_count = self.gold_count.wrapping_sub(1);
        }
        guard.gold = 0x7f;
        self.erase_guard(guard);
        let mut respawn_row = 1;
        loop {
            if respawn_row >= 16 {
                return Err("level has no empty guard resurrection cell".into());
            }
            if self.background[respawn_row * 28 + usize::from(self.resurrection_col)] == Tile::Empty
            {
                break;
            }
            self.resurrection_col += 1;
            if self.resurrection_col == 28 {
                self.resurrection_col = 0;
                respawn_row += 1;
            }
        }
        guard.col = self.resurrection_col;
        guard.row = respawn_row as u8;
        guard.x = 2;
        guard.y = 2;
        guard.animation = 0;
        guard.motion = Motion::Standing;
        self.guards.resurrection_timers[slot] = 20;
        self.store_guard(guard);
        self.add_and_update_score(0x75, 0);
        journal.push((
            Subject::Actor(ActorId::Guard(slot as u8)),
            Occurrence::Contact {
                other: Subject::Cell(Cell { col, row }),
            },
        ));
        Ok(())
    }

    pub fn guard_resurrections(&mut self) -> Result<(), String> {
        for slot in (1..=usize::from(self.guards.count)).rev() {
            if self.guards.resurrection_timers[slot] == 0 {
                continue;
            }
            let guard = self.load_guard(slot);
            self.guards.gold_timers[slot] = 0x7f;
            self.guards.resurrection_timers[slot] -= 1;
            match self.guards.resurrection_timers[slot] {
                19 | 10 => {
                    let sprite = if self.guards.resurrection_timers[slot] == 19 {
                        0x39
                    } else {
                        0x3a
                    };
                    draw_sprite(&mut self.hires, 1, sprite, guard.col, guard.row);
                    let (_, x, y) = self.guard_coordinates(guard);
                    draw_sprite_at_pixels(&mut self.hires, sprite, x, y);
                }
                0 => {
                    let index = usize::from(guard.row) * 28 + usize::from(guard.col);
                    if self.active[index] != Tile::Empty {
                        self.guards.resurrection_timers[slot] = 1;
                        continue;
                    }
                    self.active[index] = Tile::Guard;
                    draw_sprite(&mut self.hires, 1, 0, guard.col, guard.row);
                    draw_sprite(&mut self.hires, 0, 8, guard.col, guard.row);
                    self.guards.gold_timers[slot] = 0;
                    self.sound.load_sound_data(&[
                        Note {
                            duration: 2,
                            pitch: 0x7c,
                        },
                        Note {
                            duration: 3,
                            pitch: 0x78,
                        },
                        Note {
                            duration: 4,
                            pitch: 0x74,
                        },
                        Note {
                            duration: 5,
                            pitch: 0x70,
                        },
                    ])?;
                }
                _ => (),
            }
        }
        Ok(())
    }
}
