//! Incremental RGB presentation of the game's composed sprite shapes.
use crate::collision::{Layer, Scene};
use sdl2::pixels::Color;
use sdl2::pixels::PixelFormatEnum;
use sdl2::rect::Rect;
use sdl2::render::{Canvas, Texture};
use sdl2::video::Window;

pub const WIDTH: usize = 280;
pub const HEIGHT: usize = 192;
const PITCH: usize = WIDTH * 3;

/// Window snaps use the original native image dimensions in window coordinates.
pub fn scaled_size(scale: u32) -> (u32, u32) {
    (WIDTH as u32 * scale, HEIGHT as u32 * scale)
}

/// Fit the native picture geometry, leaving centered black bars.
pub fn fit_display(width: u32, height: u32) -> Rect {
    let (w, h) = if u64::from(width) * HEIGHT as u64 > u64::from(height) * WIDTH as u64 {
        (
            (u64::from(height) * WIDTH as u64 / HEIGHT as u64) as u32,
            height,
        )
    } else {
        (
            width,
            (u64::from(width) * HEIGHT as u64 / WIDTH as u64) as u32,
        )
    };
    Rect::new(
        ((width - w) / 2) as i32,
        ((height - h) / 2) as i32,
        w.max(1),
        h.max(1),
    )
}

pub struct Display {
    selected: Layer,
    rgb: Box<[u8]>,
    full_upload: bool,
    needs_present: bool,
}

impl Default for Display {
    fn default() -> Self {
        Self {
            selected: Layer::Foreground,
            rgb: vec![0; PITCH * HEIGHT].into_boxed_slice(),
            full_upload: true,
            needs_present: true,
        }
    }
}

impl Display {
    pub fn selected(&self) -> Layer {
        self.selected
    }

    pub fn select(&mut self, layer: Layer) {
        if self.selected != layer {
            self.selected = layer;
            self.full_upload = true;
            self.needs_present = true;
        }
    }

    pub fn request_present(&mut self) {
        self.needs_present = true;
    }

    // A new texture or output mode must consume every retained scene row.
    pub fn invalidate(&mut self) {
        self.full_upload = true;
        self.needs_present = true;
    }

    pub(crate) fn full_upload(&self) -> bool {
        self.full_upload
    }

    pub(crate) fn needs_present(&self) -> bool {
        self.needs_present
    }

    pub fn pixels(&self) -> &[u8] {
        &self.rgb
    }

    // Damage is grouped into neighboring cell runs, with a one-pixel horizontal
    // halo for color dependencies. Full refreshes occur only on load or selection.
    pub fn prepare(&mut self, scene: &Scene) -> Vec<Rect> {
        let regions = if self.full_upload {
            vec![Rect::new(0, 0, WIDTH as u32, HEIGHT as u32)]
        } else {
            dirty_regions(scene.dirty_cells(self.selected))
        };
        for region in &regions {
            for y in region.y() as usize..region.y() as usize + region.height() as usize {
                let bits = scene.row_bits(self.selected, y);
                for x in region.x() as usize..region.x() as usize + region.width() as usize {
                    let offset = y * PITCH + x * 3;
                    self.rgb[offset..offset + 3].copy_from_slice(&color(&bits, x));
                }
            }
        }
        regions
    }

    pub fn acknowledge(&mut self, scene: &mut Scene) {
        scene.acknowledge(self.selected);
        self.full_upload = false;
        self.needs_present = false;
    }

    // Texture updates retain their pixels between presents. The canvas is copied
    // when needed because SDL does not promise that its backbuffer survives.
    pub fn paint(
        &mut self,
        scene: &mut Scene,
        canvas: &mut Canvas<Window>,
        texture: &mut Texture<'_>,
    ) -> Result<(), String> {
        let query = texture.query();
        if query.format != PixelFormatEnum::RGB24
            || query.width != WIDTH as u32
            || query.height != HEIGHT as u32
        {
            return Err("sprite texture must be 280 by 192 in RGB24 format".into());
        }
        let regions = self.prepare(scene);
        if regions.is_empty() && !self.needs_present {
            return Ok(());
        }
        for region in regions {
            let offset = region.y() as usize * PITCH + region.x() as usize * 3;
            texture
                .update(region, &self.rgb[offset..], PITCH)
                .map_err(|error| error.to_string())?;
        }
        let (width, height) = canvas.output_size()?;
        canvas.set_draw_color(Color::BLACK);
        canvas.clear();
        canvas.copy(texture, None, fit_display(width, height))?;
        canvas.present();
        self.acknowledge(scene);
        Ok(())
    }
}

// All game assets and the border use the same original color phase. Adjacent ink
// becomes white; isolated ink and filled one-pixel gaps use global pixel parity.
fn color(bits: &[u64; 5], x: usize) -> [u8; 3] {
    let set = |x: usize| x < WIDTH && bits[x / 64] & (1 << (x % 64)) != 0;
    let here = set(x);
    let left = x > 0 && set(x - 1);
    let right = set(x + 1);
    if here && (left || right) {
        [255, 255, 255]
    } else if here || (left && right) {
        let parity = if here { x & 1 } else { (x - 1) & 1 };
        if parity == 0 {
            [32, 96, 255]
        } else {
            [255, 128, 0]
        }
    } else {
        [0, 0, 0]
    }
}

fn dirty_regions(dirty: &[bool]) -> Vec<Rect> {
    let mut regions: Vec<Rect> = Vec::new();
    for (row, cells) in dirty.as_chunks::<28>().0.iter().enumerate() {
        let mut col = 0;
        while col < 28 {
            if !cells[col] {
                col += 1;
                continue;
            }
            let start = col;
            while col < 28 && cells[col] {
                col += 1;
            }
            let x = (start * 10).saturating_sub(1);
            let right = (col * 10 + 1).min(WIDTH);
            let y = row * 11;
            let height = (HEIGHT - y).min(11);
            if let Some(previous) = regions.last_mut()
                && previous.x() == x as i32
                && previous.width() == (right - x) as u32
                && previous.y() + previous.height() as i32 == y as i32
            {
                previous.set_height(previous.height() + height as u32);
            } else {
                regions.push(Rect::new(
                    x as i32,
                    y as i32,
                    (right - x) as u32,
                    height as u32,
                ));
            }
        }
    }
    regions
}
