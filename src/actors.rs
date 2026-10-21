//! Concrete actor positions and the states that determine their next update.
use crate::animation::{GuardFrame, PlayerFrame};
use lode_trace::{ActorId, Cell, Facing, Motion};
use std::num::NonZeroU8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

impl Direction {
    pub fn motion(self) -> Motion {
        match self {
            Self::Left => Motion::Left,
            Self::Right => Motion::Right,
            Self::Up => Motion::Up,
            Self::Down => Motion::Down,
        }
    }
    pub fn horizontal(self) -> bool {
        matches!(self, Self::Left | Self::Right)
    }
}

// Each actor has five alignment positions within a cell. Boundary crossings
// move to the neighboring cell and restart at its opposite alignment edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Position {
    pub cell: Cell,
    x: u8,
    y: u8,
}

impl Position {
    pub fn centered(cell: Cell) -> Self {
        Self { cell, x: 2, y: 2 }
    }
    pub fn x(self) -> u8 {
        self.x
    }
    pub fn y(self) -> u8 {
        self.y
    }
    pub fn at_center(self) -> bool {
        self.x == 2 && self.y == 2
    }
    pub fn set_x(&mut self, x: u8) {
        assert!(x <= 4);
        self.x = x;
    }
    pub fn nudge_x(&mut self) -> bool {
        Self::nudge(&mut self.x)
    }
    pub fn nudge_y(&mut self) -> bool {
        Self::nudge(&mut self.y)
    }

    fn nudge(offset: &mut u8) -> bool {
        match (*offset).cmp(&2) {
            std::cmp::Ordering::Less => *offset += 1,
            std::cmp::Ordering::Greater => *offset -= 1,
            std::cmp::Ordering::Equal => return false,
        }
        true
    }

    pub fn step(&mut self, direction: Direction) -> bool {
        let (offset, cell_coordinate, negative) = match direction {
            Direction::Left => (&mut self.x, &mut self.cell.col, true),
            Direction::Right => (&mut self.x, &mut self.cell.col, false),
            Direction::Up => (&mut self.y, &mut self.cell.row, true),
            Direction::Down => (&mut self.y, &mut self.cell.row, false),
        };
        if negative && *offset == 0 {
            *cell_coordinate -= 1;
            *offset = 4;
            return true;
        }
        if !negative && *offset == 4 {
            *cell_coordinate += 1;
            *offset = 0;
            return true;
        }
        if negative {
            *offset -= 1;
        } else {
            *offset += 1;
        }
        false
    }

    pub fn half_x_and_pixel_y(self) -> (u8, u8) {
        const VERTICAL: [i8; 5] = [-5, -3, 0, 2, 4];
        (
            (self.cell.col * 5).wrapping_add(self.x).wrapping_sub(2),
            (self.cell.row * 11).wrapping_add_signed(VERTICAL[usize::from(self.y)]),
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DigSide {
    Left,
    Right,
}

impl DigSide {
    pub fn neighbor(self, cell: Cell) -> Option<Cell> {
        let col = match self {
            Self::Left => cell.col.checked_sub(1)?,
            Self::Right if cell.col < 27 => cell.col + 1,
            Self::Right => return None,
        };
        Some(Cell { col, row: cell.row })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Stop,
    Move(Direction),
    Dig(DigSide),
}

impl Command {
    pub fn from_key(code: u8) -> Self {
        match code {
            0xca => Self::Move(Direction::Left),
            0xcc => Self::Move(Direction::Right),
            0xc9 => Self::Move(Direction::Up),
            0xcb => Self::Move(Direction::Down),
            0xd5 => Self::Dig(DigSide::Left),
            0xcf => Self::Dig(DigSide::Right),
            _ => Self::Stop,
        }
    }
}

// Dig progress is chronological for both sides; sprite selection accounts for
// the two fixed debris sequences without exposing their table offsets as state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Digging {
    pub side: DigSide,
    stage: u8,
}

impl Digging {
    pub fn new(side: DigSide) -> Self {
        Self { side, stage: 0 }
    }
    pub fn stage(self) -> u8 {
        self.stage
    }
    pub fn advance(&mut self) {
        assert!(self.stage < 12);
        self.stage += 1;
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Player {
    pub position: Position,
    pub animation: PlayerFrame,
    pub facing: Facing,
    pub motion: Motion,
    pub digging: Option<Digging>,
    pub command: Command,
    pub alive: bool,
    pub collected_gold: bool,
}

impl Player {
    pub fn new(cell: Cell, facing: Facing) -> Self {
        Self {
            position: Position::centered(cell),
            animation: PlayerFrame::WalkRight1,
            facing,
            motion: Motion::Standing,
            digging: None,
            command: Command::Stop,
            alive: true,
            collected_gold: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GuardId(NonZeroU8);

impl GuardId {
    pub fn for_index(index: usize) -> Self {
        assert!(index < 5);
        Self(NonZeroU8::new(index as u8 + 1).expect("guard identity"))
    }
    pub fn index(self) -> usize {
        usize::from(self.0.get() - 1)
    }
    pub fn actor(self) -> ActorId {
        ActorId::Guard(self.0.get())
    }
}

// Carrying counts movement boundaries before a drop. Trapping counts selected
// updates until escape, while resurrection counts whole gameplay rounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuardMode {
    Roaming,
    Carrying { drop_in: NonZeroU8 },
    Trapped { release_in: NonZeroU8 },
    Resurrecting { appear_in: NonZeroU8 },
}

impl GuardMode {
    pub fn carrying(self) -> bool {
        matches!(self, Self::Carrying { .. })
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Guard {
    pub id: GuardId,
    pub position: Position,
    pub animation: GuardFrame,
    pub facing: Facing,
    pub mode: GuardMode,
    pub motion: Motion,
}

impl Guard {
    pub fn new(id: GuardId, cell: Cell) -> Self {
        Self {
            id,
            position: Position::centered(cell),
            animation: GuardFrame::WalkLeft1,
            facing: Facing::Right,
            mode: GuardMode::Roaming,
            motion: Motion::Standing,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct GuardSchedule {
    phase: u8,
    cursor: usize,
}

impl GuardSchedule {
    pub fn with_phase(phase: u8) -> Result<Self, String> {
        if phase >= 3 {
            return Err("invalid guard scheduler phase".into());
        }
        Ok(Self { phase, cursor: 0 })
    }
    pub fn phase(self) -> u8 {
        self.phase
    }
    pub fn updates(&mut self, count: usize, difficulty: u8) -> Result<u8, String> {
        const UPDATES: [[u8; 3]; 12] = [
            [0, 0, 0],
            [0, 1, 1],
            [1, 1, 1],
            [1, 2, 1],
            [1, 2, 2],
            [2, 2, 2],
            [2, 2, 3],
            [2, 3, 3],
            [3, 3, 3],
            [3, 3, 4],
            [3, 4, 4],
            [4, 4, 4],
        ];
        self.phase = (self.phase + 1) % 3;
        UPDATES
            .get(count + usize::from(difficulty))
            .map(|updates| updates[usize::from(self.phase)])
            .ok_or_else(|| "guard difficulty exceeds supported update counts".into())
    }
    pub fn next_guard(&mut self, count: usize) -> usize {
        let index = self.cursor % count;
        self.cursor = index + 1;
        index
    }
    pub fn restart_roster(&mut self) {
        self.cursor = 0;
    }
}
