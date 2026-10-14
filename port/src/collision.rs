//! Local shape composition for contact, independent of packed display memory.
use crate::graphics::{
    compute_shifted_sprite, draw_sprite, erase_sprite_at_pixels, overlay_sprite_at_pixels,
};
use crate::state::Game;
use std::sync::OnceLock;

const COLUMNS: usize = 28;
const ROWS: usize = 18;
const FULL: u128 = (1 << 110) - 1;
const ROW_STARTS: u128 = FULL / 1023;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Shape(u128);

impl Shape {
    pub fn sprite(sprite: u8) -> Self {
        // Sprites 102 and 103 are non-gameplay image data wider than a cell.
        // Gameplay, score text and transitions use only sprites zero through 101.
        static SHAPES: OnceLock<[Shape; 102]> = OnceLock::new();
        SHAPES.get_or_init(|| {
            std::array::from_fn(|sprite| {
                let block = compute_shifted_sprite(sprite as u8, 0);
                let mut bits = 0;
                for (row, bytes) in block.as_chunks::<3>().0.iter().enumerate() {
                    let pixels = u128::from(bytes[0] & 0x7f)
                        | u128::from(bytes[1] & 0x7f) << 7
                        | u128::from(bytes[2] & 0x7f) << 14;
                    assert!(pixels < 1024);
                    bits |= pixels << (row * 10);
                }
                Shape(bits)
            })
        })[usize::from(sprite)]
    }

    // A whole shape moves between local coordinate systems in one shift. Source
    // column clipping prevents horizontal motion from spilling into another row.
    pub fn translated(self, dx: i16, dy: i16) -> Self {
        if dx.abs() >= 10 || dy.abs() >= 11 {
            return Self::default();
        }
        let columns = if dx >= 0 {
            (1u128 << (10 - dx)) - 1
        } else {
            1023 ^ ((1u128 << -dx) - 1)
        };
        let clipped = self.0 & (columns * ROW_STARTS);
        let shift = dy * 10 + dx;
        Self(if shift >= 0 {
            (clipped << shift) & FULL
        } else {
            clipped >> -shift
        })
    }

    pub fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    fn rectangle(left: i16, top: i16, right: i16, bottom: i16) -> Self {
        let (left, top, right, bottom) = (
            left.clamp(0, 10),
            top.clamp(0, 11),
            right.clamp(0, 10),
            bottom.clamp(0, 11),
        );
        if left >= right || top >= bottom {
            return Self::default();
        }
        let rows = ((1u128 << ((bottom - top) * 10)) - 1) / 1023;
        Self((((1u128 << (right - left)) - 1) * rows) << (top * 10 + left))
    }
}

// Background and contrast retain all contact-relevant history without assigning
// pixel ownership. Contrast includes both extra ink and missing background ink.
#[derive(Clone, Copy, Debug, Default)]
struct ContactCell {
    background: Shape,
    contrast: Shape,
}

#[derive(Clone)]
pub struct Scene {
    cells: Box<[ContactCell]>,
}

impl Default for Scene {
    fn default() -> Self {
        Self {
            cells: vec![ContactCell::default(); COLUMNS * ROWS].into_boxed_slice(),
        }
    }
}

impl Scene {
    pub fn contact(&self, sprite: u8, half_x: u8, y: u8) -> bool {
        let shape = Shape::sprite(sprite);
        let (x, y) = (i16::from(half_x) * 2, i16::from(y));
        Self::nearby(x, y, 10, |index, cx, cy| {
            self.cells[index]
                .contrast
                .intersects(shape.translated(x - cx, y - cy))
        })
    }

    // Overlay replaces any contrast beneath its ink with the difference between
    // that ink and background. Terrain-covered actor pixels are not contacts.
    pub fn overlay(&mut self, sprite: u8, half_x: u8, y: u8) {
        let shape = Shape::sprite(sprite);
        let (x, y) = (i16::from(half_x) * 2, i16::from(y));
        Self::nearby(x, y, 10, |index, cx, cy| {
            let ink = shape.translated(x - cx, y - cy).0;
            let cell = &mut self.cells[index];
            cell.contrast.0 = (cell.contrast.0 & !ink) | (ink & !cell.background.0);
            false
        });
    }

    // Erasure clears its shape and restores background across the enclosing
    // seven-pixel groups. The restoration span affects empty sprites too.
    pub fn erase(&mut self, sprite: u8, half_x: u8, y: u8) {
        let shape = Shape::sprite(sprite);
        let (x, y) = (i16::from(half_x) * 2, i16::from(y));
        let left = x / 7 * 7;
        let width = if x % 7 >= 5 { 21 } else { 14 };
        Self::nearby(left, y, width, |index, cx, cy| {
            let ink = shape.translated(x - cx, y - cy).0;
            let restored = Shape::rectangle(left - cx, y - cy, left + width - cx, y + 11 - cy).0;
            let cell = &mut self.cells[index];
            cell.contrast.0 &= !(ink | (cell.background.0 & restored));
            false
        });
    }

    pub fn tile(&mut self, page: usize, sprite: u8, col: u8, row: u8) {
        let shape = Shape::sprite(sprite);
        let x = i16::from(col) * 10;
        // Status glyphs sit below the border rather than continuing the board
        // grid. Their replacement rectangle crosses two local scene rows.
        let y = if row == 16 { 181 } else { i16::from(row) * 11 };
        Self::nearby(x, y, 10, |index, cx, cy| {
            let cell = &mut self.cells[index];
            let ink = shape.translated(x - cx, y - cy).0;
            let area = Shape::rectangle(x - cx, y - cy, x + 10 - cx, y + 11 - cy).0;
            if page == 0 {
                cell.contrast.0 = (cell.contrast.0 & !area) | ((ink ^ cell.background.0) & area);
            } else {
                cell.contrast.0 ^= (cell.background.0 ^ ink) & area;
                cell.background.0 = (cell.background.0 & !area) | ink;
            }
            false
        });
    }

    pub fn clear_background(&mut self) {
        for cell in &mut self.cells {
            cell.contrast.0 ^= cell.background.0;
            cell.background = Shape::default();
        }
    }

    // The score border is foreground ink just like text. Its four rows can be
    // reached by low actor shapes, so it belongs to the contact scene as well.
    pub fn status_border(&mut self) {
        for col in 0..COLUMNS {
            let mut row = 0;
            for local_x in 0..10 {
                let x = col * 10 + local_x;
                let pattern = if (x / 7) % 2 == 0 { 0x55 } else { 0x2a };
                row |= u128::from((pattern >> (x % 7)) & 1 != 0) << local_x;
            }
            let ink = row * ((1 << 40) - 1) / 1023;
            let cell = &mut self.cells[16 * COLUMNS + col];
            let area = (1 << 40) - 1;
            cell.contrast.0 = (cell.contrast.0 & !area) | ((ink ^ cell.background.0) & area);
        }
    }

    // Only cells touched by a shape or restoration rectangle are visited. Actor
    // queries touch at most four cells and never decode or consult display bytes.
    fn nearby(x: i16, y: i16, width: i16, mut visit: impl FnMut(usize, i16, i16) -> bool) -> bool {
        if Self::in_bounds(x, y, width, &mut visit) {
            return true;
        }
        // Ink beyond the right edge reaches the first pixels 64 rows below.
        // Below row 128 it reaches unused storage and cannot affect contact.
        x + width > 280
            && Self::in_bounds(x - 280, y + 64, width, |index, cx, cy| {
                visit(index, cx + 280, cy - 64)
            })
    }

    fn in_bounds(
        mut x: i16,
        y: i16,
        width: i16,
        mut visit: impl FnMut(usize, i16, i16) -> bool,
    ) -> bool {
        let right = (x + width).min(280);
        x = x.max(0);
        let bottom = (y + 11).min(192);
        if x >= right || y >= bottom {
            return false;
        }
        for row in y / 11..=(bottom - 1) / 11 {
            for col in x / 10..=(right - 1) / 10 {
                if visit(row as usize * COLUMNS + col as usize, col * 10, row * 11) {
                    return true;
                }
            }
        }
        false
    }
}

impl Game {
    pub fn paint_tile(&mut self, page: usize, sprite: u8, col: u8, row: u8) {
        self.contacts.tile(page, sprite, col, row);
        draw_sprite(&mut self.hires, page, sprite, col, row);
    }

    pub fn overlay_shape(&mut self, sprite: u8, half_x: u8, y: u8) {
        self.contacts.overlay(sprite, half_x, y);
        overlay_sprite_at_pixels(&mut self.hires, sprite, half_x, y);
    }

    pub fn erase_shape(&mut self, sprite: u8, half_x: u8, y: u8) {
        self.contacts.erase(sprite, half_x, y);
        erase_sprite_at_pixels(&mut self.hires, sprite, half_x, y);
    }
}
