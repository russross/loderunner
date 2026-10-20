use crate::actors::{Command, DigSide, Digging, Direction};
use crate::animation::PlayerFrame;
use crate::board::Board;
use crate::collision::Layer;
use crate::devices::keyboard::{Control, RETURN};
use crate::event_timing::TimingEvent;
use crate::graphics::{DIG_BRICKS, LEFT_DIG_DEBRIS, RIGHT_DIG_DEBRIS, Sprite};
use crate::sound::{DIG_NOTES, Note, play_note};
use crate::state::Game;
use lode_trace::{ActorId, Cell, Facing, Input, Motion, Occurrence, Phase, Subject, Tile};

pub type Journal = Vec<(Subject, Occurrence)>;

impl Game {
    pub fn player_should_fall(&self) -> bool {
        let position = self.player.position;
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
            Tile::Empty => true,
            Tile::Guard => false,
            _ => !matches!(
                self.board.terrain_at(below),
                Tile::Brick | Tile::Stone | Tile::Ladder
            ),
        }
    }

    // Digging and gravity finish their update before input is eligible. A key
    // delivered during either remains latched until supported player movement.
    pub fn update_player(&mut self, journal: &mut Journal) -> Result<Vec<Input>, String> {
        self.player.collected_gold = false;
        if self.player.digging.is_some() {
            self.advance_dig(journal)?;
            return Ok(Vec::new());
        }
        if self.player_should_fall() {
            self.fall_player(journal)?;
            return Ok(Vec::new());
        }
        self.land_player(journal);
        let inputs = self.observe_player_input()?;
        if self.session.phase != Phase::Playing {
            return Ok(inputs);
        }

        // Sacrifice and level controls still finish the retained command. Pause
        // and score screens suspend before dispatching movement or a new dig.
        match self.player.command {
            Command::Stop => (),
            Command::Move(direction) => {
                self.move_player(direction, journal)?;
            }
            Command::Dig(side) => {
                self.start_dig(side, journal)?;
            }
        }
        Ok(inputs)
    }

    fn land_player(&mut self, journal: &mut Journal) {
        if self.falling_sound {
            let position = self.player.position;
            let here = self.board.terrain_at(position.cell);
            let low = if here != Tile::Ladder
                && !(here == Tile::Rope && position.y() == 2)
                && position.cell.row == 15
                && position.y() >= 2
            {
                position.cell.row
            } else {
                position.cell.col
            };
            play_note(&mut self.clock, self.sound.enabled, 0x64, 8, low);
            journal.push((Subject::Actor(ActorId::Player), Occurrence::Landed));
        }
        self.falling_sound = false;
        self.falling_pitch = 0x20;
        self.player.motion = Motion::Standing;
    }

    fn observe_player_input(&mut self) -> Result<Vec<Input>, String> {
        let Some(code) = self.keyboard.observe() else {
            return Ok(Vec::new());
        };
        match Control::from_key(code) {
            Some(Control::Pause) => self.session.phase = Phase::Paused,
            Some(Control::Sacrifice | Control::EndGame) => self.player.alive = false,
            Some(Control::SkipLevel) => {
                self.session.level = self.session.level.wrapping_add(1);
                self.skip_level = true;
                self.player.alive = false;
            }
            Some(Control::ToggleSound) => self.sound.enabled = !self.sound.enabled,
            Some(Control::Faster) => {
                self.sound.frame_period = self.sound.frame_period.saturating_sub(1)
            }
            Some(Control::Slower) => {
                self.sound.frame_period = (self.sound.frame_period + 1).min(15)
            }
            Some(Control::Keyboard) => (),
            Some(Control::Joystick) => {
                return Err("joystick controls require a controller device".into());
            }
            None if code == RETURN => (),
            None => self.player.command = Command::from_key(code),
        }
        Ok(vec![Input {
            observation: 0,
            key: code,
        }])
    }

    pub fn erase_player(&mut self) {
        let (sprite, x, y) = self.player_sprite_coordinates();
        self.erase_shape(sprite, x, y);
    }
    pub fn draw_player(&mut self) {
        let (sprite, x, y) = self.player_sprite_coordinates();
        let contact = self.scene.contact(sprite.id(), x, y);
        self.overlay_shape(sprite, x, y);
        if contact && !self.player.collected_gold {
            self.player.alive = false;
        }
    }

    // Collection is centered in both axes and suppresses overlap death for the
    // remaining player update. The actor still occupies the foreground cell.
    pub fn check_player_gold(&mut self, journal: &mut Journal) -> Result<(), String> {
        let position = self.player.position;
        if !position.at_center() || self.board.terrain_at(position.cell) != Tile::Gold {
            return Ok(());
        }
        self.player.collected_gold = true;
        self.gold.remove_one();
        self.board.terrain[Board::index(position.cell)] = Tile::Empty;
        self.paint_cell(Layer::Background, Sprite::EMPTY, position.cell);
        self.erase_cell(Sprite::GOLD, position.cell);
        self.add_score(250);
        self.sound.append_notes(&[
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
    fn center_player_x(&mut self, journal: &mut Journal) -> Result<(), String> {
        if self.player.position.nudge_x() {
            self.check_player_gold(journal)?;
        }
        Ok(())
    }
    fn center_player_y(&mut self, journal: &mut Journal) -> Result<(), String> {
        if self.player.position.nudge_y() {
            self.check_player_gold(journal)?;
        }
        Ok(())
    }

    fn fall_player(&mut self, journal: &mut Journal) -> Result<(), String> {
        self.falling_sound = true;
        self.erase_player();
        self.player.animation = if self.player.facing == Facing::Left {
            PlayerFrame::FallLeft
        } else {
            PlayerFrame::FallRight
        };
        self.center_player_x(journal)?;
        let previous = self.player.position.cell;
        if self.player.position.step(Direction::Down) {
            self.board.leave(previous);
            self.board.occupy(self.player.position.cell, Tile::Player);
        } else {
            self.check_player_gold(journal)?;
        }
        self.player.motion = Motion::Falling;
        self.draw_player();
        Ok(())
    }

    fn player_can_move(&self, direction: Direction) -> bool {
        let position = self.player.position;
        let cell = position.cell;
        let index = Board::index(cell);
        match direction {
            Direction::Left => {
                !(position.x() < 3
                    && (cell.col == 0
                        || matches!(
                            self.board.cells[index - 1],
                            Tile::Brick | Tile::Stone | Tile::Trap
                        )))
            }
            Direction::Right => {
                !(position.x() >= 2
                    && (cell.col == 27
                        || matches!(
                            self.board.cells[index + 1],
                            Tile::Brick | Tile::Stone | Tile::Trap
                        )))
            }
            Direction::Down => {
                !(position.y() >= 2
                    && (cell.row == 15
                        || matches!(self.board.cells[index + 28], Tile::Brick | Tile::Stone)))
            }
            Direction::Up => {
                if self.board.terrain[index] != Tile::Ladder {
                    position.y() > 2
                        && cell.row < 15
                        && self.board.terrain[index + 28] == Tile::Ladder
                } else {
                    !(position.y() <= 2
                        && (cell.row == 0
                            || matches!(
                                self.board.cells[index - 28],
                                Tile::Brick | Tile::Stone | Tile::Trap
                            )))
                }
            }
        }
    }

    // Every direction follows erase, perpendicular centering, travel, collection
    // and redraw. Direction-specific eligibility and animation remain explicit.
    pub fn move_player(
        &mut self,
        direction: Direction,
        journal: &mut Journal,
    ) -> Result<bool, String> {
        self.clock.events.add(TimingEvent::PlayerAttempt, 1);
        if !self.player_can_move(direction) {
            return Ok(false);
        }
        self.erase_player();
        match direction {
            Direction::Left => self.player.facing = Facing::Left,
            Direction::Right => self.player.facing = Facing::Right,
            _ => (),
        }
        if direction.horizontal() {
            self.center_player_y(journal)?;
        } else {
            self.center_player_x(journal)?;
        }
        let previous = self.player.position.cell;
        if self.player.position.step(direction) {
            self.board.leave(previous);
            self.board.occupy(self.player.position.cell, Tile::Player);
        } else {
            self.check_player_gold(journal)?;
        }
        let rope = self.board.terrain_at(self.player.position.cell) == Tile::Rope;
        self.player.animation.advance(direction, rope);
        self.player.motion = direction.motion();
        self.draw_player();
        Ok(true)
    }

    pub fn start_dig(&mut self, side: DigSide, journal: &mut Journal) -> Result<(), String> {
        self.player.command = Command::Stop;
        self.player.digging = Some(Digging::new(side));
        self.advance_dig(journal)
    }

    // A dig draws twelve stages and completes on the next update. Both sides use
    // the same progress state, with separate debris and player sprite sequences.
    fn advance_dig(&mut self, journal: &mut Journal) -> Result<(), String> {
        self.clock.events.add(TimingEvent::Dig, 1);
        let Some(mut dig) = self.player.digging else {
            return Ok(());
        };
        let cell = self.player.position.cell;
        let Some(adjacent) = dig.side.neighbor(cell).filter(|_| cell.row < 15) else {
            self.player.digging = None;
            return Ok(());
        };
        let hole = Cell {
            row: cell.row + 1,
            ..adjacent
        };
        if self.board.at(hole) != Tile::Brick {
            self.player.digging = None;
            return Ok(());
        }
        let stage = usize::from(dig.stage());
        let debris = match dig.side {
            DigSide::Left => &LEFT_DIG_DEBRIS,
            DigSide::Right => &RIGHT_DIG_DEBRIS,
        };
        if self.board.at(adjacent) != Tile::Empty {
            self.paint_cell(Layer::Foreground, Sprite::BRICK, hole);
            if dig.stage() != 0 {
                self.erase_cell(debris[stage - 1], adjacent);
            }
            self.player.digging = None;
            return Ok(());
        }
        self.erase_player();
        self.center_player_x(journal)?;
        self.center_player_y(journal)?;
        self.sound.append_note(DIG_NOTES[stage])?;
        self.player.animation = match (dig.side, dig.stage() < 6) {
            (DigSide::Left, true) => PlayerFrame::DigLeft,
            (DigSide::Left, false) => PlayerFrame::WalkLeft1,
            (DigSide::Right, true) => PlayerFrame::DigRight,
            (DigSide::Right, false) => PlayerFrame::WalkRight1,
        };
        self.draw_player();
        if dig.stage() == 12 {
            self.open_hole(hole);
            return Ok(());
        }
        if dig.stage() != 0 {
            self.erase_cell(debris[stage - 1], adjacent);
        }
        self.overlay_cell(debris[stage], adjacent);
        self.paint_cell(Layer::Foreground, DIG_BRICKS[stage], hole);
        dig.advance();
        self.player.digging = Some(dig);
        Ok(())
    }

    fn open_hole(&mut self, cell: Cell) {
        self.player.digging = None;
        self.board.occupy(cell, Tile::Empty);
        self.paint_cell(Layer::Foreground, Sprite::EMPTY, cell);
        self.paint_cell(Layer::Background, Sprite::EMPTY, cell);
        self.paint_cell(
            Layer::Foreground,
            Sprite::EMPTY,
            Cell {
                row: cell.row - 1,
                ..cell
            },
        );
        self.holes.open(cell);
    }
}
