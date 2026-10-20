use crt_native::BackendPreference;
use loderunner::collision::{Layer, Scene};
use loderunner::presentation::{CrtDisplay, Presentation, RenderMode};
use loderunner::render::{Display, fit_display, scaled_size};

fn commit(crt: &mut CrtDisplay, display: &mut Display, scene: &mut Scene) -> crt::RenderReport {
    let report = crt.prepare(scene, display).unwrap();
    crt.acknowledge(report.stamp);
    display.acknowledge(scene);
    report
}

#[test]
fn retained_crt_matches_fresh_output_across_scene_changes_layers_and_reload() {
    let mut scene = Scene::default();
    let mut display = Display::default();
    let mut crt = CrtDisplay::new(420, 288).unwrap();
    commit(&mut crt, &mut display, &mut scene);
    for step in 0..8 {
        match step {
            0 => scene.overlay(8, 50, 55),
            1 => scene.overlay(11, 55, 55),
            2 => scene.erase(8, 50, 55),
            3 => scene.overlay(37, 136, 55),
            4 => {
                scene.tile(Layer::Background, 3, 10, 5);
                display.select(Layer::Background);
            }
            5 => display.select(Layer::Foreground),
            6 => scene.tile(Layer::Foreground, 69, 10, 16),
            7 => {
                scene = Scene::default();
                display = Display::default();
            }
            _ => unreachable!(),
        }
        let report = commit(&mut crt, &mut display, &mut scene);
        if step == 0 {
            assert!(report.stats.decoded_samples < 192 * 1824 / 4);
            assert!(report.stats.reconstructed_pixels < 420 * 288 / 4);
        }
        let mut fresh = CrtDisplay::new(420, 288).unwrap();
        fresh.prepare(&scene, &display).unwrap();
        assert_eq!(crt.pixels(), fresh.pixels(), "step {step}");
    }
    assert!(
        commit(&mut crt, &mut display, &mut scene)
            .changed
            .is_empty()
    );
}

// Cell edges and HGR fetch edges do not coincide. Check retained output through
// disconnected damage, erasure, the status strip, and both screen edges.
#[test]
fn cell_damage_spans_preserve_hgr_neighbors_and_untouched_rows() {
    let mut scene = Scene::default();
    let mut display = Display::default();
    for row in 0..17 {
        for column in 0..28 {
            scene.tile(Layer::Foreground, (row + column) % 8, column, row);
        }
    }
    let mut crt = CrtDisplay::new(280, 192).unwrap();
    commit(&mut crt, &mut display, &mut scene);
    for (x, y) in [(0, 0), (3, 10), (50, 55), (136, 55), (0, 176), (134, 181)] {
        for erase in [false, true] {
            if erase {
                scene.erase(9, x, y);
            } else {
                scene.overlay(9, x, y);
            }
            commit(&mut crt, &mut display, &mut scene);
            let mut fresh = CrtDisplay::new(280, 192).unwrap();
            fresh.prepare(&scene, &display).unwrap();
            assert_eq!(crt.pixels(), fresh.pixels(), "{x},{y}, erase={erase}");
        }
    }
}

#[test]
fn resize_reconstructs_retained_light_and_centers_the_native_aspect() {
    assert_eq!(scaled_size(5), (1400, 960));
    for scale in 1..=5 {
        let (w, h) = scaled_size(scale);
        assert_eq!(fit_display(w, h), sdl2::rect::Rect::new(0, 0, w, h));
    }
    assert_eq!(
        fit_display(1000, 192),
        sdl2::rect::Rect::new(360, 0, 280, 192)
    );
    assert_eq!(
        fit_display(280, 500),
        sdl2::rect::Rect::new(0, 154, 280, 192)
    );
    let mut scene = Scene::default();
    let mut display = Display::default();
    scene.overlay(9, 51, 55);
    let mut crt = CrtDisplay::new(280, 192).unwrap();
    commit(&mut crt, &mut display, &mut scene);
    crt.resize(701, 333).unwrap();
    let report = commit(&mut crt, &mut display, &mut scene);
    assert_eq!(report.stats.decoded_samples, 0);
    assert!(report.stats.full_redraw);
    assert!(
        crt.pixels()
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[..3] != [0; 3])
    );
    for row in crt.pixels().as_chunks::<{ 701 * 4 }>().0 {
        assert_eq!(&row[..4], &[0, 0, 0, 255]);
        assert_eq!(&row[row.len() - 4..], &[0, 0, 0, 255]);
    }
}

#[test]
#[ignore = "run through make test-render-sdl with the dummy SDL driver"]
fn sdl_simple_retains_rgba_through_local_updates_and_exposure() {
    let sdl = sdl2::init().unwrap();
    let video = sdl.video().unwrap();
    let window = video
        .window("simple retention", 280, 192)
        .hidden()
        .build()
        .unwrap();
    let mut presentation =
        Presentation::new(window, RenderMode::Simple, BackendPreference::Cpu).unwrap();
    let mut scene = Scene::default();
    let mut display = Display::default();
    for step in 0..7 {
        match step {
            0 => scene.overlay(8, 50, 55),
            1 => scene.overlay(11, 55, 55),
            2 => scene.erase(8, 50, 55),
            3 => scene.overlay(37, 136, 55),
            4 => scene.tile(Layer::Foreground, 69, 10, 16),
            5 => display.request_present(),
            6 => {
                scene.tile(Layer::Background, 3, 10, 5);
                display.select(Layer::Background);
            }
            _ => unreachable!(),
        }
        presentation.paint(&mut scene, &mut display).unwrap();
        let actual = presentation.capture_cpu().unwrap();
        let mut fresh = Display::default();
        fresh.select(display.selected());
        fresh.prepare(&scene);
        for (rgb, rgba) in fresh
            .pixels()
            .as_chunks::<3>()
            .0
            .iter()
            .zip(actual.as_chunks::<4>().0)
        {
            assert_eq!(rgb, &rgba[..3], "step {step}");
        }
    }
}

#[test]
#[ignore = "run through make test-render-sdl with the dummy SDL driver"]
fn sdl_crt_uploads_and_mode_switches_preserve_pixels_and_letterboxing() {
    let sdl = sdl2::init().unwrap();
    let video = sdl.video().unwrap();
    let window = video
        .window("CRT presentation test", 420, 288)
        .hidden()
        .resizable()
        .build()
        .unwrap();
    let mut presentation =
        Presentation::new(window, RenderMode::Crt, BackendPreference::Cpu).unwrap();
    let mut scene = Scene::default();
    let mut display = Display::default();
    for step in 0..7 {
        match step {
            0 => scene.overlay(8, 50, 55),
            1 => scene.overlay(11, 55, 55),
            2 => {
                presentation.window_mut().set_size(600, 288).unwrap();
                display.request_present();
            }
            3 => {
                scene.tile(Layer::Background, 3, 10, 5);
                display.select(Layer::Background);
            }
            4 => {
                scene = Scene::default();
                display = Display::default();
            }
            5 => {
                scene.overlay(8, 50, 55);
                presentation.toggle(&mut display);
            }
            6 => presentation.toggle(&mut display),
            _ => unreachable!(),
        }
        presentation.paint(&mut scene, &mut display).unwrap();
        let (width, height) = presentation.size();
        let actual = presentation.capture_cpu().unwrap();
        if step != 5 {
            let mut fresh = CrtDisplay::new(width, height).unwrap();
            fresh.prepare(&scene, &display).unwrap();
            assert_eq!(actual, fresh.pixels(), "step {step}");
        } else {
            let rect = fit_display(width, height);
            // Simple source stays at native resolution; side bars remain black.
            for y in 0..height as usize {
                assert_eq!(&actual[y * width as usize * 4..][..3], &[0; 3]);
            }
            assert!(rect.width() < width);
            assert!(actual.as_chunks::<4>().0.iter().any(|p| p[..3] != [0; 3]));
        }
    }
}
