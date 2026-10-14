use crate::IMAGE;
use crate::devices::hires::Hires;
use crate::event_timing::{Draw, DrawKind};

const MASK0: [u8; 7] = [0, 1, 3, 7, 15, 31, 63];
const MASK1: [u8; 7] = [0xf8, 0xf0, 0xe0, 0xc0, 0x80, 0xfe, 0xfc];

pub fn row_address(page: usize, row: u8) -> u16 {
    let low = u16::from(IMAGE[0x1a85 + usize::from(row)]);
    let high = u16::from(IMAGE[0x1b45 + usize::from(row)]) | if page == 0 { 0x20 } else { 0x40 };
    high << 8 | low
}

// Sprite data is stored by row-byte plane with a stride of 104 sprites. Lookup
// tables retain the original pattern mapping and phase bits without code patching.
pub fn compute_shifted_sprite(sprite: u8, shift: u8) -> [u8; 33] {
    assert!(sprite < 104 && shift < 7);
    let page = usize::from(IMAGE[0x84c1 + usize::from(shift)]) << 8;
    let mut block = [0; 33];
    for (row, pixels) in block.as_chunks_mut::<3>().0.iter_mut().enumerate() {
        // The two source planes share the middle output byte. Shift patterns
        // already include the phase bits needed for hires artifact colors.
        for plane in 0..2 {
            let pattern =
                usize::from(IMAGE[0xad00 + (row * 2 + plane) * 104 + usize::from(sprite)]);
            let address =
                usize::from(IMAGE[page + pattern]) | usize::from(IMAGE[page + 0x80 + pattern]) << 8;
            pixels[plane] |= IMAGE[address];
            pixels[plane + 1] |= IMAGE[address + 1];
        }
    }
    block
}

pub fn draw_sprite(hires: &mut Hires, page: usize, sprite: u8, col: u8, row: u8) {
    assert!(page < 2 && col < 28 && row < 17);
    let y = IMAGE[0x1c51 + usize::from(row)];
    let byte = IMAGE[0x1c62 + usize::from(col)];
    let shift = IMAGE[0x1c7e + usize::from(col)];
    let block = compute_shifted_sprite(sprite, shift);
    let width = if shift >= 5 { 3 } else { 2 };
    hires.events.draw(Draw {
        kind: DrawKind::Tile,
        sprite,
        half_x: col * 5,
        y,
        shift,
    });
    for (line, pixels) in block.as_chunks::<3>().0.iter().enumerate() {
        let address = row_address(page, y + line as u8) + u16::from(byte);
        for (column, &bits) in pixels.iter().take(width).enumerate() {
            let mask = if column == 0 {
                MASK0[usize::from(shift)]
            } else if column == width - 1 {
                MASK1[usize::from(shift)]
            } else {
                0
            };
            let value = hires.read(address + column as u16) & mask | bits;
            hires.write(address + column as u16, value);
        }
    }
}

// Actor drawing reports the original foreground/background overlap bits before
// merging the sprite. The caller applies gameplay's gold-pickup exception.
pub fn draw_sprite_at_pixels(hires: &mut Hires, sprite: u8, half_x: u8, y: u8) -> u8 {
    let byte = IMAGE[0x1c9a + usize::from(half_x)];
    let shift = IMAGE[0x1d26 + usize::from(half_x)];
    let block = compute_shifted_sprite(sprite, shift);
    let width = if shift >= 5 { 3 } else { 2 };
    hires.events.draw(Draw {
        kind: DrawKind::Actor,
        sprite,
        half_x,
        y,
        shift,
    });
    let mut overlap = 0;
    for (line, pixels) in block.as_chunks::<3>().0.iter().enumerate() {
        let address = row_address(0, y + line as u8) + u16::from(byte);
        for (column, &bits) in pixels.iter().take(width).enumerate() {
            let address = address + column as u16;
            let foreground = hires.read(address);
            let background = hires.read(address + 0x2000);
            overlap |= (foreground ^ background) & bits;
            hires.write(address, foreground | bits);
        }
    }
    overlap
}

pub fn erase_sprite_at_pixels(hires: &mut Hires, sprite: u8, half_x: u8, y: u8) {
    let byte = IMAGE[0x1c9a + usize::from(half_x)];
    let shift = IMAGE[0x1d26 + usize::from(half_x)];
    let block = compute_shifted_sprite(sprite, shift);
    let width = if shift >= 5 { 3 } else { 2 };
    hires.events.draw(Draw {
        kind: DrawKind::Erase,
        sprite,
        half_x,
        y,
        shift,
    });
    for (line, pixels) in block.as_chunks::<3>().0.iter().enumerate() {
        let address = row_address(0, y + line as u8) + u16::from(byte);
        for (column, &bits) in pixels.iter().take(width).enumerate() {
            let address = address + column as u16;
            let value = hires.read(address) & (bits ^ 0x7f) | hires.read(address + 0x2000);
            hires.write(address, value);
        }
    }
}
