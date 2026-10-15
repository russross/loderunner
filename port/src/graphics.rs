//! Sprite assets and the ordered appearance operations of gameplay.
use crate::assets::SPRITE_ROWS;
use crate::collision::Layer;
use crate::event_timing::{Draw, DrawKind};
use crate::state::Game;
use lode_trace::{Cell, Tile};

// Sprite identities select reference geometry and timing, independently of the
// RGB presentation. Only the asset boundary deals with their original numbers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sprite(u8);

impl Sprite {
    pub const EMPTY: Self = Self(0);
    pub const BRICK: Self = Self(1);
    pub const LADDER: Self = Self(3);
    pub const GOLD: Self = Self(7);
    pub const REFILL_FIRST: Self = Self(55);
    pub const REFILL_SECOND: Self = Self(56);
    pub const RESURRECT_FIRST: Self = Self(57);
    pub const RESURRECT_SECOND: Self = Self(58);

    pub(crate) const fn reference(id: u8) -> Self {
        assert!(id < 102);
        Self(id)
    }
    pub fn id(self) -> u8 {
        self.0
    }

    // Trap tiles look like brick, while other initial markers retain their
    // reference shapes. Hidden ladders are removed before board composition.
    pub fn tile(tile: Tile) -> Self {
        if tile == Tile::Trap {
            Self::BRICK
        } else {
            Self::reference(tile as u8)
        }
    }

    pub fn glyph(byte: u8) -> Self {
        Self(match byte {
            b'A'..=b'Z' => byte - b'A' + 69,
            b'0'..=b'9' => byte - b'0' + 59,
            b'>' => 95,
            b'.' => 96,
            b'(' => 97,
            b')' => 98,
            b'/' => 99,
            b'-' => 100,
            b'<' => 101,
            _ => 0,
        })
    }
}

pub const LEFT_DIG_DEBRIS: [Sprite; 12] = [
    Sprite(27),
    Sprite(27),
    Sprite(28),
    Sprite(28),
    Sprite(29),
    Sprite(29),
    Sprite(30),
    Sprite(30),
    Sprite::EMPTY,
    Sprite::EMPTY,
    Sprite::EMPTY,
    Sprite::EMPTY,
];
pub const RIGHT_DIG_DEBRIS: [Sprite; 12] = [
    Sprite(38),
    Sprite(38),
    Sprite(39),
    Sprite(39),
    Sprite(29),
    Sprite(29),
    Sprite(30),
    Sprite(30),
    Sprite::EMPTY,
    Sprite::EMPTY,
    Sprite::EMPTY,
    Sprite::EMPTY,
];
pub const DIG_BRICKS: [Sprite; 12] = [
    Sprite(31),
    Sprite(31),
    Sprite(32),
    Sprite(32),
    Sprite(33),
    Sprite(33),
    Sprite(34),
    Sprite(34),
    Sprite(35),
    Sprite(35),
    Sprite(36),
    Sprite(36),
];

// The build converts reference sprites to unshifted native ink rows. Drawing
// and contact share this geometry while the timing model retains sprite IDs.
pub fn sprite_rows(sprite: u8) -> [u16; 11] {
    SPRITE_ROWS[usize::from(sprite)]
}

impl Game {
    pub fn paint_cell(&mut self, layer: Layer, sprite: Sprite, cell: Cell) {
        self.paint_tile(layer, sprite, cell.col, cell.row);
    }

    pub fn overlay_cell(&mut self, sprite: Sprite, cell: Cell) {
        self.overlay_shape(sprite, cell.col * 5, cell.row * 11);
    }

    pub fn erase_cell(&mut self, sprite: Sprite, cell: Cell) {
        self.erase_shape(sprite, cell.col * 5, cell.row * 11);
    }

    // Status text uses logical row sixteen at its separate pixel baseline.
    // Board-cell callers use the cell helpers above rather than pixel units.
    pub fn paint_tile(&mut self, layer: Layer, sprite: Sprite, col: u8, row: u8) {
        self.scene.tile(layer, sprite.id(), col, row);
        self.record_draw(
            DrawKind::Tile,
            sprite,
            col * 5,
            if row == 16 { 181 } else { row * 11 },
        );
    }

    pub fn overlay_shape(&mut self, sprite: Sprite, half_x: u8, y: u8) {
        self.scene.overlay(sprite.id(), half_x, y);
        self.record_draw(DrawKind::Actor, sprite, half_x, y);
    }

    pub fn erase_shape(&mut self, sprite: Sprite, half_x: u8, y: u8) {
        self.scene.erase(sprite.id(), half_x, y);
        self.record_draw(DrawKind::Erase, sprite, half_x, y);
    }

    // These inputs describe the original machine's work for the cycle model.
    // Presentation consumes only changed scene ink and does not add model work.
    fn record_draw(&mut self, kind: DrawKind, sprite: Sprite, half_x: u8, y: u8) {
        self.clock.record_draw(Draw {
            kind,
            sprite: sprite.id(),
            half_x,
            y,
        });
    }
}
