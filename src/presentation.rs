//! Host output selection, retained NTSC rendering, and aspect-preserving sizing.
use crate::collision::Scene;
use crate::render::{Display, HEIGHT, WIDTH};
use crate::transition::VisualEffect;
use crt::source::apple2::{AppleEncoder, AppleLine, AppleProfile};
use crt::{
    CompositeConfig, OutputBufferMut, PixelFormat, RasterLayout, RenderStamp, Renderer, Sampling,
    SignalLineBuffer, ViewConfig,
};
use crt_native::{BackendPreference, Display as NativeDisplay};
use sdl2::video::Window;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RenderMode {
    #[default]
    Crt,
    Simple,
}
impl RenderMode {
    pub fn name(self) -> &'static str {
        match self {
            Self::Crt => "crt",
            Self::Simple => "simple",
        }
    }
    pub fn toggled(self) -> Self {
        match self {
            Self::Crt => Self::Simple,
            Self::Simple => Self::Crt,
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
        self.submit(scene, display, None)?;
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
    fn submit(
        &mut self,
        scene: &Scene,
        display: &Display,
        host: Option<&NativeDisplay>,
    ) -> Result<(), String> {
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
            let line = self.signal.as_line().map_err(|e| e.to_string())?;
            if let Some(host) = host {
                host.submit_line(&mut self.renderer, y as u32, line)
            } else {
                self.renderer.submit_line(y as u32, line)
            }
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    pub fn acknowledge(&mut self, stamp: RenderStamp) {
        self.stamp = Some(stamp);
        self.full_source = false;
    }
}

/// The native presenter owns either a direct GPU surface or the SDL fallback.
pub struct Presentation {
    host: NativeDisplay,
    mode: RenderMode,
    crt: Option<CrtDisplay>,
    simple: Vec<u8>,
    effect: VisualEffect,
}
impl Presentation {
    pub fn new(
        window: Window,
        mode: RenderMode,
        preference: BackendPreference,
    ) -> Result<Self, String> {
        Ok(Self {
            host: NativeDisplay::new(window, preference)?,
            mode,
            crt: None,
            simple: vec![0; WIDTH * HEIGHT * 4],
            effect: VisualEffect::None,
        })
    }
    pub fn window_mut(&mut self) -> &mut Window {
        self.host.window_mut()
    }
    pub fn size(&self) -> (u32, u32) {
        self.host.size()
    }
    pub fn is_gpu(&self) -> bool {
        self.host.is_gpu()
    }
    pub fn capture_cpu(&mut self) -> Result<Vec<u8>, String> {
        self.host.capture_cpu()
    }
    pub fn snapshot(&mut self) -> Result<Vec<u8>, String> {
        let crt = self.crt.as_mut().ok_or("no CRT picture")?;
        self.host.snapshot(&mut crt.renderer)
    }
    pub fn toggle(&mut self, display: &mut Display) {
        self.mode = self.mode.toggled();
        self.host.invalidate();
        if let Some(crt) = &mut self.crt {
            crt.full_source = true;
        }
        display.invalidate();
    }
    pub fn paint(&mut self, scene: &mut Scene, display: &mut Display) -> Result<(), String> {
        self.paint_effect(scene, display, VisualEffect::None)
    }

    pub fn paint_effect(
        &mut self,
        scene: &mut Scene,
        display: &mut Display,
        effect: VisualEffect,
    ) -> Result<(), String> {
        if self.effect != effect {
            self.effect = effect;
            display.invalidate();
        }
        if effect == VisualEffect::None {
            return self.paint_scene(scene, display);
        }
        let mut picture = scene.clone();
        effect.apply(&mut picture);
        self.paint_scene(&mut picture, display)?;
        display.acknowledge(scene);
        Ok(())
    }

    fn paint_scene(&mut self, scene: &mut Scene, display: &mut Display) -> Result<(), String> {
        let (width, height) = self.host.size();
        if width == 0 || height == 0 {
            return Ok(());
        }
        if self.mode == RenderMode::Simple {
            let regions = display.prepare(scene);
            if !regions.is_empty() || display.needs_present() || self.host.needs_present() {
                for (rgb, rgba) in display
                    .pixels()
                    .chunks_exact(3)
                    .zip(self.simple.chunks_exact_mut(4))
                {
                    rgba[..3].copy_from_slice(rgb);
                    rgba[3] = 255;
                }
                self.host.present_rgba(
                    &self.simple,
                    WIDTH as u32,
                    HEIGHT as u32,
                    WIDTH as f32 / HEIGHT as f32,
                )?;
                display.acknowledge(scene);
            }
            return Ok(());
        }
        if self.crt.is_none() {
            self.crt = Some(CrtDisplay::new(width, height)?);
        }
        let crt = self.crt.as_mut().expect("CRT output initialized");
        crt.resize(width, height)?;
        crt.submit(scene, display, Some(&self.host))?;
        self.host
            .present_crt(&mut crt.renderer, display.needs_present())?;
        crt.full_source = false;
        display.acknowledge(scene);
        Ok(())
    }
}
