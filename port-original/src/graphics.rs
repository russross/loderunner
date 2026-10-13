use crate::IMAGE;
use crate::devices::hires::Hires;

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
    let mut pointer = 0xad00;
    for row in 0..11 {
        let first = usize::from(IMAGE[pointer + usize::from(sprite)]);
        let address =
            usize::from(IMAGE[page + first]) | usize::from(IMAGE[page + 0x80 + first]) << 8;
        block[row * 3] = IMAGE[address];
        block[row * 3 + 1] = IMAGE[address + 1];
        pointer += 104;

        let second = usize::from(IMAGE[pointer + usize::from(sprite)]);
        let address =
            usize::from(IMAGE[page + second]) | usize::from(IMAGE[page + 0x80 + second]) << 8;
        block[row * 3 + 1] |= IMAGE[address];
        block[row * 3 + 2] = IMAGE[address + 1];
        pointer += 104;
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
    for line in 0..11 {
        let address = row_address(page, y + line as u8) + u16::from(byte);
        for column in 0..width {
            let mask = if column == 0 {
                MASK0[usize::from(shift)]
            } else if column == width - 1 {
                MASK1[usize::from(shift)]
            } else {
                0
            };
            let value = hires.read(address + column as u16) & mask | block[line * 3 + column];
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
    let mut overlap = 0;
    for line in 0..11 {
        let address = row_address(0, y + line as u8) + u16::from(byte);
        for column in 0..width {
            let address = address + column as u16;
            let bits = block[line * 3 + column];
            overlap |= (hires.read(address) ^ hires.read(address + 0x2000)) & bits;
            hires.write(address, hires.read(address) | bits);
        }
    }
    overlap
}

pub fn erase_sprite_at_pixels(hires: &mut Hires, sprite: u8, half_x: u8, y: u8) {
    let byte = IMAGE[0x1c9a + usize::from(half_x)];
    let shift = IMAGE[0x1d26 + usize::from(half_x)];
    let block = compute_shifted_sprite(sprite, shift);
    let width = if shift >= 5 { 3 } else { 2 };
    for line in 0..11 {
        let address = row_address(0, y + line as u8) + u16::from(byte);
        for column in 0..width {
            let address = address + column as u16;
            let value = hires.read(address) & (block[line * 3 + column] ^ 0x7f)
                | hires.read(address + 0x2000);
            hires.write(address, value);
        }
    }
}
