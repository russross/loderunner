use crate::actors::{Direction, Guard, GuardMode, Position};
use crate::animation::GuardFrame;
use crate::board::Board;
use crate::collision::Layer;
use crate::event_timing::TimingEvent;
use crate::graphics::Sprite;
use crate::player::Journal;
use crate::routing;
use crate::sound::Note;
use crate::state::Game;
use lode_trace::{ActorId, Cell, Facing, Motion, Occurrence, Subject, Tile};
use std::num::NonZeroU8;

impl Game {
    fn guard_coordinates(&self, guard: Guard) -> (Sprite, u8, u8) {
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
        let route = routing::choose(&self.board, self.player.position.cell, *guard);
        self.clock.events.search(route.work);
        self.move_guard(guard, route.direction, journal);
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
            drop_in: self
                .recording_overrides
                .gold_delay(NonZeroU8::new(self.resurrection_col + 1).expect("gold drop delay")),
        };
        self.board.terrain[Board::index(cell)] = Tile::Empty;
        self.paint_cell(Layer::Background, Sprite::EMPTY, cell);
        self.erase_cell(Sprite::GOLD, cell);
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
        self.paint_cell(Layer::Background, Sprite::GOLD, cell);
        self.overlay_cell(Sprite::GOLD, cell);
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
                    self.paint_cell(Layer::Background, Sprite::GOLD, previous);
                    self.overlay_cell(Sprite::GOLD, previous);
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
                self.sound.append_notes(&[
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
        if let Some(column) = self.recording_overrides.resurrection_column() {
            self.resurrection_col = column;
        }
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
        for index in (0..self.guards.len()).rev() {
            let mut guard = self.guards[index];
            let GuardMode::Resurrecting { appear_in } = guard.mode else {
                continue;
            };
            let remaining = appear_in.get() - 1;
            let cell = guard.position.cell;
            if remaining == 0 {
                if self.board.at(cell) != Tile::Empty {
                    continue;
                }
                self.board.occupy(cell, Tile::Guard);
                self.paint_cell(Layer::Background, Sprite::EMPTY, cell);
                self.paint_cell(Layer::Foreground, GuardFrame::WalkLeft1.sprite(), cell);
                guard.mode = GuardMode::Roaming;
                self.sound.append_notes(&[
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
                    let sprite = if remaining == 19 {
                        Sprite::RESURRECT_FIRST
                    } else {
                        Sprite::RESURRECT_SECOND
                    };
                    self.paint_cell(Layer::Background, sprite, cell);
                    self.overlay_cell(sprite, cell);
                }
            }
            self.guards[index] = guard;
        }
        Ok(())
    }
}
