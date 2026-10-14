//! Sprite assets and the ordered appearance operations of gameplay.
use crate::IMAGE;
use crate::collision::Layer;
use crate::event_timing::{Draw, DrawKind};
use crate::state::Game;

pub const GUARD_SPRITES: [u8; 16] = [
    8, 43, 44, 48, 49, 50, 54, 40, 41, 42, 45, 46, 47, 53, 51, 52,
];
pub const LEFT_DIG_DEBRIS: [u8; 12] = [27, 27, 28, 28, 29, 29, 30, 30, 0, 0, 0, 0];
pub const RIGHT_DIG_DEBRIS: [u8; 12] = [38, 38, 39, 39, 29, 29, 30, 30, 0, 0, 0, 0];
pub const DIG_BRICKS: [u8; 12] = [31, 31, 32, 32, 33, 33, 34, 34, 35, 35, 36, 36];

// The assets encode two seven-bit planes through pattern indices. Decode their
// unshifted ink once; drawing and contact share the resulting local geometry.
pub fn sprite_rows(sprite: u8) -> [u16; 11] {
    assert!(sprite < 102);
    let patterns = usize::from(IMAGE[0x84c1]) << 8;
    std::array::from_fn(|row| {
        let mut pixels = 0;
        for plane in 0..2 {
            let pattern =
                usize::from(IMAGE[0xad00 + (row * 2 + plane) * 104 + usize::from(sprite)]);
            let address = usize::from(IMAGE[patterns + pattern])
                | usize::from(IMAGE[patterns + 0x80 + pattern]) << 8;
            let ink = u16::from(IMAGE[address] & 0x7f) | u16::from(IMAGE[address + 1] & 0x7f) << 7;
            pixels |= ink << (plane * 7);
        }
        assert!(pixels < 1024);
        pixels
    })
}

impl Game {
    pub fn paint_tile(&mut self, layer: Layer, sprite: u8, col: u8, row: u8) {
        self.scene.tile(layer, sprite, col, row);
        self.record_draw(
            DrawKind::Tile,
            sprite,
            col * 5,
            if row == 16 { 181 } else { row * 11 },
        );
    }

    pub fn overlay_shape(&mut self, sprite: u8, half_x: u8, y: u8) {
        self.scene.overlay(sprite, half_x, y);
        self.record_draw(DrawKind::Actor, sprite, half_x, y);
    }

    pub fn erase_shape(&mut self, sprite: u8, half_x: u8, y: u8) {
        self.scene.erase(sprite, half_x, y);
        self.record_draw(DrawKind::Erase, sprite, half_x, y);
    }

    // These inputs describe the original machine's work for the cycle model.
    // Presentation consumes only changed scene ink and does not add model work.
    fn record_draw(&mut self, kind: DrawKind, sprite: u8, half_x: u8, y: u8) {
        self.display.events.draw(Draw {
            kind,
            sprite,
            half_x,
            y,
            shift: (u16::from(half_x) * 2 % 7) as u8,
        });
    }
}
