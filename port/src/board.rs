//! The fixed playfield and ordered holes that temporarily remove its bricks.
use lode_trace::{Cell, HolePhase, Tile};
use std::num::NonZeroU8;

pub struct Board {
    pub terrain: [Tile; 448],
    pub cells: [Tile; 448],
}

impl Board {
    pub fn new(tiles: [Tile; 448]) -> Self {
        Self {
            terrain: tiles,
            cells: tiles,
        }
    }
    pub fn index(cell: Cell) -> usize {
        usize::from(cell.row) * 28 + usize::from(cell.col)
    }
    pub fn terrain_at(&self, cell: Cell) -> Tile {
        self.terrain[Self::index(cell)]
    }
    pub fn at(&self, cell: Cell) -> Tile {
        self.cells[Self::index(cell)]
    }
    pub fn occupy(&mut self, cell: Cell, actor: Tile) {
        self.cells[Self::index(cell)] = actor;
    }

    // An actor leaving a dug brick reveals the opening, not its retained brick
    // terrain. Other cells recover their terrain, including dropped gold.
    pub fn leave(&mut self, cell: Cell) {
        let terrain = self.terrain_at(cell);
        self.occupy(
            cell,
            if terrain == Tile::Brick {
                Tile::Empty
            } else {
                terrain
            },
        );
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpenHole {
    pub cell: Cell,
    pub remaining: NonZeroU8,
    pub generation: u32,
}

impl OpenHole {
    pub fn phase(self) -> HolePhase {
        match self.remaining.get() {
            1..=10 => HolePhase::ClosingSecond,
            11..=20 => HolePhase::ClosingFirst,
            _ => HolePhase::Open,
        }
    }
}

// Holes refill in reverse allocation order. Free timer positions are reused from
// the beginning, while generations distinguish repeat openings of the same cell.
pub struct Holes {
    pub entries: [Option<OpenHole>; 30],
    generations: [u32; 448],
}

impl Default for Holes {
    fn default() -> Self {
        Self {
            entries: [None; 30],
            generations: [0; 448],
        }
    }
}

impl Holes {
    pub fn open(&mut self, cell: Cell) {
        if let Some(entry) = self.entries.iter_mut().find(|entry| entry.is_none()) {
            let generation = &mut self.generations[Board::index(cell)];
            *generation += 1;
            *entry = Some(OpenHole {
                cell,
                remaining: NonZeroU8::new(180).expect("hole duration"),
                generation: *generation,
            });
        }
    }
}
