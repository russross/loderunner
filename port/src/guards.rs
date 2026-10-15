use crate::actors::{Direction, Guard, GuardMode, Position};
use crate::animation::GuardFrame;
use crate::board::Board;
use crate::collision::Layer;
use crate::event_timing::TimingEvent;
use crate::player::Journal;
use crate::sound::Note;
use crate::state::Game;
use lode_trace::{ActorId, Cell, Facing, Motion, Occurrence, Subject, Tile};
use std::num::NonZeroU8;

struct RouteChoice {
    distance: u8,
    direction: Option<Direction>,
}

impl Game {
    fn guard_coordinates(&self, guard: Guard) -> (u8, u8, u8) {
        let (half_x, y) = guard.position.half_x_and_pixel_y();
        (guard.animation.sprite(), half_x, y)
    }
    fn erase_guard(&mut self, guard: Guard) {
        let (sprite, x, y) = self.guard_coordinates(guard);
        self.erase_shape(sprite, x, y);
    }
    fn draw_guard(&mut self, guard: Guard) {
        let (sprite, x, y) = self.guard_coordinates(guard);
        self.overlay_shape(sprite, x, y);
    }

    // Difficulty selects a three-phase update count. Selected identities rotate
    // through the concrete guards, stopping immediately when the player dies.
    pub fn update_guards(&mut self, journal: &mut Journal) -> Result<(), String> {
        if self.guards.is_empty() {
            return Ok(());
        }
        let updates = self
            .guard_schedule
            .updates(self.guards.len(), self.difficulty)?;
        for _ in 0..updates {
            let index = self.guard_schedule.next_guard(self.guards.len());
            let mut guard = self.guards[index];
            guard.motion = Motion::Standing;
            self.update_guard(&mut guard, journal)?;
            self.guards[index] = guard;
            if !self.player.alive {
                break;
            }
        }
        Ok(())
    }

    fn update_guard(&mut self, guard: &mut Guard, journal: &mut Journal) -> Result<(), String> {
        self.clock.events.add(TimingEvent::GuardUpdate, 1);
        match guard.mode {
            GuardMode::Resurrecting { .. } => return Ok(()),
            GuardMode::Trapped { release_in } => {
                let remaining = release_in.get() - 1;
                guard.mode = NonZeroU8::new(remaining).map_or(GuardMode::Roaming, |release_in| {
                    GuardMode::Trapped { release_in }
                });
                if remaining >= 7 {
                    if remaining < 13 {
                        self.erase_guard(*guard);
                        guard
                            .position
                            .set_x([2, 1, 2, 3, 2, 1][usize::from(remaining - 7)]);
                    }
                    self.draw_guard(*guard);
                    return Ok(());
                }
            }
            _ if self.guard_should_fall(*guard) => return self.fall_guard(guard, journal),
            _ => (),
        }
        let direction = self.choose_guard_route(*guard);
        self.move_guard(guard, direction, journal);
        Ok(())
    }

    fn guard_should_fall(&self, guard: Guard) -> bool {
        let position = guard.position;
        let cell = position.cell;
        let here = self.board.terrain_at(cell);
        if here == Tile::Ladder || (here == Tile::Rope && position.y() == 2) {
            return false;
        }
        if position.y() < 2 {
            return true;
        }
        if cell.row == 15 {
            return false;
        }
        let below = Cell {
            row: cell.row + 1,
            ..cell
        };
        match self.board.at(below) {
            Tile::Empty | Tile::Player => true,
            Tile::Guard => false,
            _ => !matches!(
                self.board.terrain_at(below),
                Tile::Brick | Tile::Stone | Tile::Ladder
            ),
        }
    }

    fn check_guard_gold(&mut self, guard: &mut Guard, journal: &mut Journal) {
        let cell = guard.position.cell;
        if !guard.position.at_center()
            || guard.mode.carrying()
            || self.board.terrain_at(cell) != Tile::Gold
        {
            return;
        }
        guard.mode = GuardMode::Carrying {
            drop_in: NonZeroU8::new(self.resurrection_col + 1).expect("gold drop delay"),
        };
        self.board.terrain[Board::index(cell)] = Tile::Empty;
        self.paint_tile(Layer::Background, 0, cell.col, cell.row);
        self.erase_shape(7, cell.col * 5, cell.row * 11);
        journal.push((Subject::Actor(guard.id.actor()), Occurrence::GoldCollected));
    }

    pub fn drop_guard_gold(&mut self, guard: &mut Guard, journal: &mut Journal) {
        let GuardMode::Carrying { drop_in } = guard.mode else {
            return;
        };
        if let Some(remaining) = NonZeroU8::new(drop_in.get() - 1) {
            guard.mode = GuardMode::Carrying { drop_in: remaining };
            return;
        }
        let cell = guard.position.cell;
        if self.board.terrain_at(cell) != Tile::Empty {
            return;
        }
        guard.mode = GuardMode::Roaming;
        self.board.terrain[Board::index(cell)] = Tile::Gold;
        self.paint_tile(Layer::Background, 7, cell.col, cell.row);
        self.overlay_shape(7, cell.col * 5, cell.row * 11);
        journal.push((
            Subject::Actor(guard.id.actor()),
            Occurrence::GoldDropped { cell },
        ));
    }

    fn center_guard_x(&mut self, guard: &mut Guard, journal: &mut Journal) {
        if guard.position.nudge_x() {
            self.check_guard_gold(guard, journal);
        }
    }
    fn center_guard_y(&mut self, guard: &mut Guard, journal: &mut Journal) {
        if guard.position.nudge_y() {
            self.check_guard_gold(guard, journal);
        }
    }
    fn occupy_guard_cell(&mut self, guard: Guard, journal: &mut Journal) {
        if self.board.at(guard.position.cell) == Tile::Player {
            self.player.alive = false;
            journal.push((
                Subject::Actor(ActorId::Player),
                Occurrence::Contact {
                    other: Subject::Actor(guard.id.actor()),
                },
            ));
        }
        self.board.occupy(guard.position.cell, Tile::Guard);
    }

    // Falling into brick drops carried gold above the hole before centering.
    // Centering starts the trap delay, awards points and queues the trap tune.
    fn fall_guard(&mut self, guard: &mut Guard, journal: &mut Journal) -> Result<(), String> {
        self.clock.events.add(TimingEvent::GuardFall, 1);
        self.erase_guard(*guard);
        self.center_guard_x(guard, journal);
        guard.animation = if guard.facing == Facing::Left {
            GuardFrame::FallLeft
        } else {
            GuardFrame::FallRight
        };
        let previous = guard.position.cell;
        let crossed = guard.position.step(Direction::Down);
        guard.motion = Motion::Falling;
        if crossed {
            self.board.leave(previous);
            if self.board.at(guard.position.cell) == Tile::Player {
                self.player.alive = false;
                journal.push((
                    Subject::Actor(ActorId::Player),
                    Occurrence::Contact {
                        other: Subject::Actor(guard.id.actor()),
                    },
                ));
            }
            if self.board.terrain_at(guard.position.cell) == Tile::Brick && guard.mode.carrying() {
                if self.board.terrain_at(previous) == Tile::Empty {
                    self.board.terrain[Board::index(previous)] = Tile::Gold;
                    self.board.occupy(previous, Tile::Gold);
                    self.paint_tile(Layer::Background, 7, previous.col, previous.row);
                    self.overlay_shape(7, previous.col * 5, previous.row * 11);
                    journal.push((
                        Subject::Actor(guard.id.actor()),
                        Occurrence::GoldDropped { cell: previous },
                    ));
                } else {
                    self.gold.remove_one();
                }
                guard.mode = GuardMode::Roaming;
            }
            self.board.occupy(guard.position.cell, Tile::Guard);
        } else if guard.position.y() == 2 {
            self.check_guard_gold(guard, journal);
            if self.board.terrain_at(guard.position.cell) == Tile::Brick {
                if guard.mode.carrying() {
                    self.gold.remove_one();
                }
                let delay =
                    [38, 38, 46, 68, 71, 73, 74, 75, 76, 77, 78][usize::from(self.difficulty)];
                guard.mode = NonZeroU8::new(delay).map_or(GuardMode::Roaming, |release_in| {
                    GuardMode::Trapped { release_in }
                });
                self.add_score(75);
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
                journal.push((Subject::Actor(guard.id.actor()), Occurrence::Trapped));
            }
        }
        self.draw_guard(*guard);
        Ok(())
    }

    // Guards have their own obstacle rules, while positions share cell crossing
    // with the player. A blocked climb extends an unfinished escape by one tick.
    pub fn move_guard(
        &mut self,
        guard: &mut Guard,
        direction: Option<Direction>,
        journal: &mut Journal,
    ) {
        self.clock.events.add(TimingEvent::GuardAttempt, 1);
        let Some(direction) = direction else {
            return;
        };
        let position = guard.position;
        let cell = position.cell;
        let index = Board::index(cell);
        let blocked = match direction {
            Direction::Left => {
                position.x() < 3
                    && (cell.col == 0
                        || matches!(
                            self.board.cells[index - 1],
                            Tile::Guard | Tile::Stone | Tile::Brick
                        )
                        || self.board.terrain[index - 1] == Tile::Trap)
            }
            Direction::Right => {
                position.x() >= 2
                    && (cell.col == 27
                        || matches!(
                            self.board.cells[index + 1],
                            Tile::Guard | Tile::Stone | Tile::Brick
                        )
                        || self.board.terrain[index + 1] == Tile::Trap)
            }
            Direction::Up => {
                position.y() < 3
                    && (cell.row == 0
                        || matches!(
                            self.board.cells[index - 28],
                            Tile::Brick | Tile::Stone | Tile::Trap | Tile::Guard
                        ))
            }
            Direction::Down => {
                position.y() >= 2
                    && (cell.row == 15
                        || matches!(
                            self.board.cells[index + 28],
                            Tile::Brick | Tile::Stone | Tile::Guard
                        ))
            }
        };
        if blocked {
            if direction == Direction::Up
                && let GuardMode::Trapped { release_in } = guard.mode
            {
                guard.mode = GuardMode::Trapped {
                    release_in: NonZeroU8::new(release_in.get() + 1).expect("escape delay"),
                };
            }
            return;
        }
        self.erase_guard(*guard);
        if direction.horizontal() {
            self.center_guard_y(guard, journal);
        } else {
            self.center_guard_x(guard, journal);
        }
        match direction {
            Direction::Left => guard.facing = Facing::Left,
            Direction::Right => guard.facing = Facing::Right,
            _ => (),
        }
        let previous = guard.position.cell;
        let offset = if direction.horizontal() {
            guard.position.x()
        } else {
            guard.position.y()
        };
        let crossing = match direction {
            Direction::Left | Direction::Up => offset == 0,
            _ => offset == 4,
        };
        if crossing {
            self.drop_guard_gold(guard, journal);
        }
        guard.position.step(direction);
        guard.motion = direction.motion();
        if crossing {
            self.board.leave(previous);
            self.occupy_guard_cell(*guard, journal);
        } else {
            self.check_guard_gold(guard, journal);
        }
        let rope = self.board.terrain_at(guard.position.cell) == Tile::Rope;
        guard.animation.advance(direction, rope);
        self.draw_guard(*guard);
    }
    fn choose_guard_route(&mut self, guard: Guard) -> Option<Direction> {
        self.clock.events.add(TimingEvent::GuardRoute, 1);
        let index =
            usize::from(guard.position.cell.row) * 28 + usize::from(guard.position.cell.col);
        if self.board.terrain[index] == Tile::Brick
            && matches!(guard.mode, GuardMode::Trapped { .. })
        {
            return Some(Direction::Up);
        }
        if guard.position.cell.row == self.player.position.cell.row
            && guard.position.cell.col != self.player.position.cell.col
        {
            let direction = if guard.position.cell.col < self.player.position.cell.col {
                1_i16
            } else {
                -1
            };
            let mut col = i16::from(guard.position.cell.col);
            let mut clear = true;
            while col != i16::from(self.player.position.cell.col) {
                self.clock.events.add(TimingEvent::SameRowScan, 1);
                col += direction;
                let index = usize::from(guard.position.cell.row) * 28 + col as usize;
                if !matches!(self.board.terrain[index], Tile::Ladder | Tile::Rope)
                    && guard.position.cell.row != 15
                {
                    self.clock.events.add(TimingEvent::SameRowSupport, 1);
                }
                if !matches!(self.board.terrain[index], Tile::Ladder | Tile::Rope)
                    && guard.position.cell.row != 15
                    && matches!(self.board.terrain[index + 28], Tile::Empty | Tile::Trap)
                {
                    clear = false;
                    break;
                }
            }
            if clear {
                return Some(if direction > 0 {
                    Direction::Right
                } else {
                    Direction::Left
                });
            }
        }
        self.clock.events.add(TimingEvent::HorizontalSearch, 1);
        let (left, right) = self.guard_horizontal_limits(guard);
        self.clock
            .events
            .add(TimingEvent::LimitScan, u64::from(right - left));
        let mut best = RouteChoice {
            distance: u8::MAX,
            direction: None,
        };
        self.consider_guard_routes(
            guard,
            guard.position.cell.col,
            Direction::Down,
            Direction::Up,
            &mut best,
        );
        for col in left..guard.position.cell.col {
            self.consider_guard_routes(guard, col, Direction::Left, Direction::Left, &mut best);
        }
        for col in (guard.position.cell.col + 1..=right).rev() {
            self.consider_guard_routes(guard, col, Direction::Right, Direction::Right, &mut best);
        }
        best.direction
    }

    fn consider_guard_routes(
        &mut self,
        guard: Guard,
        col: u8,
        down: Direction,
        up: Direction,
        best: &mut RouteChoice,
    ) {
        self.clock.events.add(TimingEvent::RouteCandidate, 1);

        let index = usize::from(guard.position.cell.row) * 28 + usize::from(col);
        if guard.position.cell.row < 15
            && !matches!(self.board.terrain[index + 28], Tile::Brick | Tile::Stone)
        {
            self.clock.events.add(TimingEvent::RouteDown, 1);
            let row = self.guard_candidate_below(col, guard.position.cell.row);
            let distance = self.guard_distance(guard, col, row);
            if distance < best.distance {
                self.clock.events.add(TimingEvent::BetterRoute, 1);
                best.distance = distance;
                best.direction = Some(down);
            }
        }
        if guard.position.cell.row > 0 && self.board.terrain[index] == Tile::Ladder {
            self.clock.events.add(TimingEvent::RouteUp, 1);
            let row = self.guard_candidate_above(col, guard.position.cell.row);
            let distance = self.guard_distance(guard, col, row);
            if distance < best.distance {
                self.clock.events.add(TimingEvent::BetterRoute, 1);
                best.distance = distance;
                best.direction = Some(up);
            }
        }
    }

    // Reaching the player's row outranks routes above, then routes below. On the
    // same row, a nearer candidate column outranks a farther one relative to the guard.
    fn guard_distance(&mut self, guard: Guard, col: u8, row: u8) -> u8 {
        if row == self.player.position.cell.row {
            self.clock.events.add(TimingEvent::DistanceSameRow, 1);
            col.abs_diff(guard.position.cell.col)
        } else if row > self.player.position.cell.row {
            self.clock.events.add(TimingEvent::DistanceBelow, 1);
            200 + row - self.player.position.cell.row
        } else {
            self.clock.events.add(TimingEvent::DistanceAbove, 1);
            100 + self.player.position.cell.row - row
        }
    }

    fn guard_horizontal_limits(&self, guard: Guard) -> (u8, u8) {
        let traversable = |col: u8| {
            let index = usize::from(guard.position.cell.row) * 28 + usize::from(col);
            if matches!(self.board.cells[index], Tile::Brick | Tile::Stone) {
                return (false, false);
            }
            (
                true,
                matches!(self.board.cells[index], Tile::Ladder | Tile::Rope)
                    || guard.position.cell.row == 15
                    || matches!(
                        self.board.terrain[index + 28],
                        Tile::Brick | Tile::Stone | Tile::Ladder
                    ),
            )
        };
        let mut left = guard.position.cell.col;
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
        let mut right = guard.position.cell.col;
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
            self.clock.events.add(TimingEvent::AboveScan, 1);

            let old = usize::from(row) * 28 + usize::from(col);
            if self.board.terrain[old] != Tile::Ladder {
                return row;
            }
            row -= 1;
            let index = usize::from(row) * 28 + usize::from(col);
            self.clock.events.add(
                TimingEvent::AbovePlayerProbe,
                u64::from(row <= self.player.position.cell.row),
            );
            self.clock
                .events
                .add(TimingEvent::AboveLeftProbe, u64::from(col > 0));
            if col > 0
                && (matches!(
                    self.board.terrain[old - 1],
                    Tile::Brick | Tile::Stone | Tile::Ladder
                ) || self.board.terrain[index - 1] == Tile::Rope)
                && row <= self.player.position.cell.row
            {
                return row;
            }
            self.clock
                .events
                .add(TimingEvent::AboveRightProbe, u64::from(col < 27));
            if col < 27
                && (matches!(
                    self.board.terrain[old + 1],
                    Tile::Brick | Tile::Stone | Tile::Ladder
                ) || self.board.terrain[index + 1] == Tile::Rope)
                && row <= self.player.position.cell.row
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
            self.clock.events.add(TimingEvent::BelowScan, 1);

            if row == 15 {
                return 15;
            }
            let index = usize::from(row) * 28 + usize::from(col);
            if matches!(self.board.terrain[index + 28], Tile::Brick | Tile::Stone) {
                return row;
            }
            let mut right_neighbor_row = row;
            if self.board.terrain[index] != Tile::Empty {
                self.clock.events.add(TimingEvent::BelowNonEmpty, 1);
                self.clock.events.add(
                    TimingEvent::BelowPlayerProbe,
                    u64::from(row >= self.player.position.cell.row),
                );
                if col > 0 {
                    self.clock.events.add(TimingEvent::BelowLeftProbe, 1);
                    if self.board.terrain[index - 1] == Tile::Rope
                        && row >= self.player.position.cell.row
                    {
                        return row;
                    }
                    if self.board.terrain[index - 1] != Tile::Rope {
                        right_neighbor_row = row + 1;
                        if matches!(
                            self.board.terrain[index + 27],
                            Tile::Brick | Tile::Stone | Tile::Ladder
                        ) && row >= self.player.position.cell.row
                        {
                            return row;
                        }
                    }
                }
                if col < 27 {
                    self.clock.events.add(TimingEvent::BelowRightProbe, 1);
                    // A left neighbor without a rope moves the right-neighbor
                    // check down one row. Otherwise that check stays on this row.
                    let pointer = usize::from(right_neighbor_row) * 28 + usize::from(col);
                    if self.board.terrain[pointer + 1] == Tile::Rope
                        && row >= self.player.position.cell.row
                    {
                        return row;
                    }
                    if self.board.terrain[pointer + 1] != Tile::Rope
                        && matches!(
                            self.board.terrain[index + 29],
                            Tile::Brick | Tile::Stone | Tile::Ladder
                        )
                        && row >= self.player.position.cell.row
                    {
                        return row;
                    }
                }
            }
            row += 1;
        }
    }

    pub fn bury_guard(&mut self, cell: Cell, journal: &mut Journal) -> Result<(), String> {
        let index = self
            .guards
            .iter()
            .rposition(|guard| guard.position.cell == cell)
            .ok_or("guard occupancy has no matching guard")?;
        let mut guard = self.guards[index];
        if guard.mode.carrying() {
            self.gold.remove_one();
        }
        self.erase_guard(guard);
        let mut row = 1;
        // Resurrection scans empty terrain by column then row. Occupancy is tested
        // separately when the delay expires, allowing a blocked spawn to retry.
        loop {
            if row >= 16 {
                return Err("level has no empty guard resurrection cell".into());
            }
            if self.board.terrain_at(Cell {
                col: self.resurrection_col,
                row,
            }) == Tile::Empty
            {
                break;
            }
            self.resurrection_col += 1;
            if self.resurrection_col == 28 {
                self.resurrection_col = 0;
                row += 1;
            }
        }
        guard.position = Position::centered(Cell {
            col: self.resurrection_col,
            row,
        });
        guard.animation = GuardFrame::WalkLeft1;
        guard.motion = Motion::Standing;
        guard.mode = GuardMode::Resurrecting {
            appear_in: NonZeroU8::new(20).expect("resurrection duration"),
        };
        self.guards[index] = guard;
        self.add_score(75);
        journal.push((
            Subject::Actor(guard.id.actor()),
            Occurrence::Contact {
                other: Subject::Cell(cell),
            },
        ));
        Ok(())
    }

    pub fn update_resurrections(&mut self) -> Result<(), String> {
        self.clock.events.add(TimingEvent::ResurrectionScan, 1);
        for index in (0..self.guards.len()).rev() {
            let mut guard = self.guards[index];
            let GuardMode::Resurrecting { appear_in } = guard.mode else {
                continue;
            };
            self.clock.events.add(TimingEvent::ActiveResurrection, 1);
            let remaining = appear_in.get() - 1;
            let cell = guard.position.cell;
            if remaining == 0 {
                if self.board.at(cell) != Tile::Empty {
                    continue;
                }
                self.board.occupy(cell, Tile::Guard);
                self.paint_tile(Layer::Background, 0, cell.col, cell.row);
                self.paint_tile(Layer::Foreground, 8, cell.col, cell.row);
                guard.mode = GuardMode::Roaming;
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
            } else {
                guard.mode = GuardMode::Resurrecting {
                    appear_in: NonZeroU8::new(remaining).expect("pending resurrection"),
                };
                if matches!(remaining, 19 | 10) {
                    let sprite = if remaining == 19 { 0x39 } else { 0x3a };
                    self.paint_tile(Layer::Background, sprite, cell.col, cell.row);
                    let (x, y) = guard.position.half_x_and_pixel_y();
                    self.overlay_shape(sprite, x, y);
                }
            }
            self.guards[index] = guard;
        }
        Ok(())
    }
}
