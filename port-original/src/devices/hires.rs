use sdl2::pixels::PixelFormatEnum;
use sdl2::rect::Rect;
use sdl2::render::{Canvas, Texture};
use sdl2::video::Window;

pub const WIDTH: usize = 280;
pub const HEIGHT: usize = 192;

pub struct Hires {
    pages: Box<[[u8; 8192]; 2]>,
    selected: usize,
    dirty: [[bool; HEIGHT]; 2],
    address_row: Box<[u8]>,
    pending_cycles: u64,
}

impl Default for Hires {
    fn default() -> Self {
        let mut address_row = vec![0xff; 8192].into_boxed_slice();
        for row in 0..HEIGHT {
            for col in 0..40 {
                address_row[Self::row_offset(row) + col] = row as u8;
            }
        }
        Self {
            pages: Box::new([[0x80; 8192]; 2]),
            selected: 0,
            dirty: [[true; HEIGHT]; 2],
            address_row,
            pending_cycles: 0,
        }
    }
}

impl Hires {
    pub fn account_cycles(&mut self, cycles: u64) {
        self.pending_cycles += cycles;
    }

    pub fn take_cycles(&mut self) -> u64 {
        std::mem::take(&mut self.pending_cycles)
    }

    pub fn row_offset(row: usize) -> usize {
        (row & 7) * 0x400 + ((row >> 3) & 7) * 0x80 + (row >> 6) * 40
    }

    pub fn read(&self, address: u16) -> u8 {
        let offset = usize::from(address.checked_sub(0x2000).expect("hires address"));
        self.pages[offset / 8192][offset % 8192]
    }

    // Every byte persists, including holes. Visible writes invalidate a whole
    // scanline so artifact-color dependencies across byte boundaries are covered.
    pub fn write(&mut self, address: u16, value: u8) {
        let offset = usize::from(address.checked_sub(0x2000).expect("hires address"));
        let page = offset / 8192;
        let offset = offset % 8192;
        if self.pages[page][offset] != value {
            self.pages[page][offset] = value;
            let row = self.address_row[offset];
            if row != 0xff {
                self.dirty[page][usize::from(row)] = true;
            }
        }
    }

    pub fn select(&mut self, page: usize) {
        assert!(page < 2);
        if page != self.selected {
            self.selected = page;
            self.dirty[page].fill(true);
        }
    }

    pub fn softswitch(&mut self, address: u16) {
        match address {
            0xc054 => self.select(0),
            0xc055 => self.select(1),
            0xc050 | 0xc052 | 0xc057 => (),
            _ => panic!("unsupported graphics softswitch {address:04x}"),
        }
    }

    pub fn clear(&mut self, page: usize) {
        for offset in 0..8192 {
            self.write((0x2000 + page * 8192 + offset) as u16, 0x80);
        }
    }

    pub fn dirty_rows(&self) -> Vec<usize> {
        self.dirty[self.selected]
            .iter()
            .enumerate()
            .filter_map(|(row, dirty)| dirty.then_some(row))
            .collect()
    }

    pub fn acknowledge(&mut self) {
        self.dirty[self.selected].fill(false);
    }

    // This digital artifact decoder preserves the original bit plane for collision.
    // Isolated alternating patterns produce color; adjacent set bits produce white.
    pub fn rgb_row(&self, row: usize) -> Vec<u8> {
        let bytes = &self.pages[self.selected][Self::row_offset(row)..Self::row_offset(row) + 40];
        let set = |x: usize| x < WIDTH && bytes[x / 7] & (1 << (x % 7)) != 0;
        let mut rgb = Vec::with_capacity(WIDTH * 3);
        for x in 0..WIDTH {
            let left = x > 0 && set(x - 1);
            let right = set(x + 1);
            let color = if set(x) && (left || right) {
                [255, 255, 255]
            } else if set(x) || (left && right) {
                let parity = if set(x) { x & 1 } else { (x - 1) & 1 };
                match (bytes[x / 7] & 0x80 != 0, parity) {
                    (true, 0) => [32, 96, 255],
                    (true, _) => [255, 128, 0],
                    (false, 0) => [192, 32, 255],
                    (false, _) => [32, 224, 64],
                }
            } else {
                [0, 0, 0]
            };
            rgb.extend(color);
        }
        rgb
    }

    pub fn paint(
        &mut self,
        canvas: &mut Canvas<Window>,
        texture: &mut Texture<'_>,
    ) -> Result<(), String> {
        if texture.query().format != PixelFormatEnum::RGB24 {
            return Err("hires texture must use RGB24".into());
        }
        for row in self.dirty_rows() {
            texture
                .update(
                    Rect::new(0, row as i32, WIDTH as u32, 1),
                    &self.rgb_row(row),
                    WIDTH * 3,
                )
                .map_err(|error| error.to_string())?;
        }
        canvas.copy(texture, None, None)?;
        canvas.present();
        self.acknowledge();
        Ok(())
    }
}
