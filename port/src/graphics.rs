//! Sprite assets and the ordered appearance operations of gameplay.
use crate::assets::SPRITE_ROWS;
use crate::collision::Layer;
use crate::event_timing::{Draw, DrawKind};
use crate::state::Game;

pub const LEFT_DIG_DEBRIS: [u8; 12] = [27, 27, 28, 28, 29, 29, 30, 30, 0, 0, 0, 0];
pub const RIGHT_DIG_DEBRIS: [u8; 12] = [38, 38, 39, 39, 29, 29, 30, 30, 0, 0, 0, 0];
pub const DIG_BRICKS: [u8; 12] = [31, 31, 32, 32, 33, 33, 34, 34, 35, 35, 36, 36];

// The build converts reference sprites to unshifted native ink rows. Drawing
// and contact share this geometry while the timing model retains sprite IDs.
pub fn sprite_rows(sprite: u8) -> [u16; 11] {
    SPRITE_ROWS[usize::from(sprite)]
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
        self.clock.record_draw(Draw {
            kind,
            sprite,
            half_x,
            y,
        });
    }
}
