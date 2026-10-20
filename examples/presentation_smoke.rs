//! Exercise real native presentation, resizing, mode switches, and retained damage.
use crt_native::BackendPreference;
use loderunner::collision::{Layer, Scene};
use loderunner::presentation::{CrtDisplay, Presentation, RenderMode};
use loderunner::render::Display;
use loderunner::transition::{IRIS_FRAMES, VisualEffect};
use std::io::Write;

fn run() -> Result<(), String> {
    let sdl = sdl2::init()?;
    let video = sdl.video()?;
    let window = video
        .window("Lode Runner presentation smoke", 701, 477)
        .resizable()
        .allow_highdpi()
        .metal_view()
        .build()
        .map_err(|e| e.to_string())?;
    let preference = BackendPreference::from_env()?;
    let mut presentation = Presentation::new(window, RenderMode::Crt, preference)?;
    if preference == BackendPreference::Gpu && !presentation.is_gpu() {
        return Err("GPU required".into());
    }
    let mut events = sdl.event_pump()?;
    let mut scene = Scene::default();
    let mut display = Display::default();
    for row in 0..17 {
        for column in 0..28 {
            scene.tile(Layer::Foreground, (row + column) % 8, column, row);
        }
    }
    for step in 0..12 {
        events.pump_events();
        if step == 2 {
            presentation
                .window_mut()
                .set_size(601, 333)
                .map_err(|e| e.to_string())?;
        }
        if step == 4 || step == 5 {
            presentation.toggle(&mut display);
        }
        scene.overlay(8, 50 + step * 2, 55);
        display.request_present();
        let effect = match step {
            8 => VisualEffect::Iris { frame: 0 },
            9 => VisualEffect::Iris {
                frame: IRIS_FRAMES / 2,
            },
            10 => VisualEffect::Iris { frame: IRIS_FRAMES },
            _ => VisualEffect::None,
        };
        presentation.paint_effect(&mut scene, &mut display, effect)?;
        if step == 4 {
            continue;
        }
        let (width, height) = presentation.size();
        let actual = presentation.snapshot()?;
        let mut expected_scene = scene.clone();
        effect.apply(&mut expected_scene);
        let mut reference = CrtDisplay::new(width, height)?;
        reference.prepare(&expected_scene, &display)?;
        let max = actual
            .iter()
            .zip(reference.pixels())
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap_or(0);
        if max > 2 {
            return Err(format!("step {step}: GPU/CPU difference {max}"));
        }
        if step == 7 || step == 9 {
            let path = if step == 7 {
                "target/presentation-smoke.ppm"
            } else {
                "target/iris-smoke.ppm"
            };
            let mut file = std::fs::File::create(path).map_err(|e| e.to_string())?;
            write!(file, "P6\n{width} {height}\n255\n").map_err(|e| e.to_string())?;
            let rgb: Vec<_> = actual
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|p| p[..3].iter().copied())
                .collect();
            file.write_all(&rgb).map_err(|e| e.to_string())?;
        }
        // A second paint exercises unchanged output after successful presentation.
        presentation.paint(&mut scene, &mut display)?;
    }
    println!(
        "Lode Runner {:?}: partial updates, resize, exposure, mode switches, and CPU image comparison passed",
        preference
    );
    Ok(())
}
fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("presentation-smoke: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
