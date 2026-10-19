//! Host output selection, retained NTSC rendering, and aspect-preserving sizing.
use crate::collision::Scene;
use crate::render::{Display, HEIGHT, WIDTH};
use crt::source::apple2::{AppleEncoder, AppleLine, AppleProfile};
use crt::{
    CompositeConfig, OutputBufferMut, PixelFormat, RasterLayout, RenderStamp, Renderer, Sampling,
    SignalLineBuffer, ViewConfig,
};
use sdl2::pixels::PixelFormatEnum;
use sdl2::rect::Rect;
use sdl2::render::{Canvas, Texture, TextureCreator};
use sdl2::video::{Window, WindowContext};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RenderMode {
    #[default]
    Crt,
    Classic,
}
impl RenderMode {
    pub fn name(self) -> &'static str {
        match self {
            Self::Crt => "CRT",
            Self::Classic => "classic",
        }
    }
    pub fn toggled(self) -> Self {
        match self {
            Self::Crt => Self::Classic,
            Self::Classic => Self::Crt,
        }
    }
}

/// CRT storage belongs to the host and survives source reloads and window resizes.
pub struct CrtDisplay {
    renderer: Renderer,
    encoder: AppleEncoder,
    signal: SignalLineBuffer,
    pixels: Vec<u8>,
    stamp: Option<RenderStamp>,
    size: (u32, u32),
    full_source: bool,
}
impl CrtDisplay {
    pub fn new(width: u32, height: u32) -> Result<Self, String> {
        let renderer = Renderer::new_composite(CompositeConfig::default(), RasterLayout::apple2())
            .map_err(|e| e.to_string())?;
        let mut result = Self {
            renderer,
            encoder: AppleEncoder::new(AppleProfile::IieNtscV1, Sampling::Eight),
            signal: SignalLineBuffer::new(),
            pixels: Vec::new(),
            stamp: None,
            size: (0, 0),
            full_source: true,
        };
        result.resize(width, height)?;
        Ok(result)
    }
    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), String> {
        if self.size == (width, height) {
            return Ok(());
        }
        let mut view = ViewConfig::new(width, height, RasterLayout::apple2());
        // Preserve the native game's geometry rather than stretching on resize.
        view.reference_aspect = WIDTH as f32 / HEIGHT as f32;
        self.renderer.set_view(view).map_err(|e| e.to_string())?;
        self.pixels.resize(width as usize * height as usize * 4, 0);
        self.size = (width, height);
        self.stamp = None;
        Ok(())
    }
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    // Scene damage selects rows; the signal renderer finds horizontal changes and
    // propagates both electrical and optical dependencies into destination pixels.
    pub fn prepare(
        &mut self,
        scene: &Scene,
        display: &Display,
    ) -> Result<crt::RenderReport, String> {
        let dirty = scene.dirty_cells(display.selected());
        for y in 0..HEIGHT {
            if !self.full_source
                && !display.full_upload()
                && !dirty[(y / 11) * 28..(y / 11 + 1) * 28].iter().any(|v| *v)
            {
                continue;
            }
            let ink = scene.row_bits(display.selected(), y);
            let bytes = std::array::from_fn(|byte| {
                let mut value = 0x80;
                for bit in 0..7 {
                    let x = byte * 7 + bit;
                    value |= (((ink[x / 64] >> (x % 64)) & 1) as u8) << bit;
                }
                value
            });
            self.encoder
                .encode_line(&AppleLine::hires(&bytes), &mut self.signal)
                .map_err(|e| e.to_string())?;
            self.renderer
                .submit_line(y as u32, self.signal.as_line().map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        }
        self.renderer
            .render_into(
                OutputBufferMut::new(
                    &mut self.pixels,
                    self.size.0,
                    self.size.1,
                    self.size.0 as usize * 4,
                    PixelFormat::Rgba8,
                )
                .map_err(|e| e.to_string())?,
                self.stamp,
            )
            .map_err(|e| e.to_string())
    }
    pub fn acknowledge(&mut self, stamp: RenderStamp) {
        self.stamp = Some(stamp);
        self.full_source = false;
    }
}

/// SDL textures retain uploads; client CPU buffers remain authoritative on errors.
pub struct Presentation<'a> {
    creator: &'a TextureCreator<WindowContext>,
    mode: RenderMode,
    texture: Option<Texture<'a>>,
    crt: Option<CrtDisplay>,
}
impl<'a> Presentation<'a> {
    pub fn new(creator: &'a TextureCreator<WindowContext>, mode: RenderMode) -> Self {
        Self {
            creator,
            mode,
            texture: None,
            crt: None,
        }
    }
    pub fn toggle(&mut self, display: &mut Display) {
        self.mode = self.mode.toggled();
        self.texture = None;
        self.crt = None;
        display.invalidate();
    }
    pub fn paint(
        &mut self,
        scene: &mut Scene,
        display: &mut Display,
        canvas: &mut Canvas<Window>,
    ) -> Result<(), String> {
        let (width, height) = canvas.output_size()?;
        if width == 0 || height == 0 {
            return Ok(());
        }
        let (format, tw, th) = match self.mode {
            RenderMode::Classic => (PixelFormatEnum::RGB24, WIDTH as u32, HEIGHT as u32),
            RenderMode::Crt => (PixelFormatEnum::RGBA32, width, height),
        };
        let recreate = self.texture.as_ref().is_none_or(|t| {
            let q = t.query();
            q.width != tw || q.height != th || q.format != format
        });
        if recreate {
            self.texture = Some(
                self.creator
                    .create_texture_streaming(format, tw, th)
                    .map_err(|e| e.to_string())?,
            );
            if self.mode == RenderMode::Classic {
                display.invalidate();
            }
        }
        let texture = self
            .texture
            .as_mut()
            .expect("texture created for current mode");
        if self.mode == RenderMode::Classic {
            return display.paint(scene, canvas, texture);
        }
        if self.crt.is_none() {
            self.crt = Some(CrtDisplay::new(width, height)?);
        }
        let crt = self.crt.as_mut().expect("CRT output initialized");
        crt.resize(width, height)?;
        let report = crt.prepare(scene, display)?;
        if report.changed.is_empty() && !display.needs_present() && !recreate {
            return Ok(());
        }
        let pitch = width as usize * 4;
        for region in &report.changed {
            let rect = Rect::new(
                region.x as i32,
                region.y as i32,
                region.width,
                region.height,
            );
            let offset = region.y as usize * pitch + region.x as usize * 4;
            texture
                .update(rect, &crt.pixels()[offset..], pitch)
                .map_err(|e| e.to_string())?;
        }
        canvas.copy(texture, None, None)?;
        canvas.present();
        crt.acknowledge(report.stamp);
        display.acknowledge(scene);
        Ok(())
    }
}
