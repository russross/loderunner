use lode_original::devices::hires::Hires;
use lode_original::graphics::{draw_sprite_at_pixels, erase_sprite_at_pixels};
use lode_port::collision::{Layer, Scene};
use lode_port::render::{Display, HEIGHT, WIDTH};
use sdl2::pixels::PixelFormatEnum;
use sdl2::rect::Rect;

fn refreshed(scene: &mut Scene) -> Display {
    let mut display = Display::default();
    assert_eq!(display.prepare(scene), vec![Rect::new(0, 0, 280, 192)]);
    display.acknowledge(scene);
    display
}

// A normal sprite change updates a small local rectangle. Repeated overlay is
// idempotent, and erasing it restores the retained texture pixels to background.
#[test]
fn sprite_updates_are_local_and_unchanged_ink_has_no_damage() {
    let mut scene = Scene::default();
    let mut display = refreshed(&mut scene);
    scene.overlay(9, 51, 55);
    assert_eq!(display.prepare(&scene), vec![Rect::new(99, 55, 22, 11)]);
    assert!(display.pixels().iter().any(|&byte| byte != 0));
    display.acknowledge(&mut scene);
    scene.overlay(9, 51, 55);
    assert!(display.prepare(&scene).is_empty());
    scene.erase(9, 51, 55);
    assert_eq!(display.prepare(&scene), vec![Rect::new(99, 55, 22, 11)]);
    assert!(display.pixels().iter().all(|&byte| byte == 0));
}

#[test]
fn hidden_background_changes_do_not_dirty_foreground() {
    let mut scene = Scene::default();
    let mut display = refreshed(&mut scene);
    scene.tile(Layer::Background, 1, 10, 5);
    assert!(display.prepare(&scene).is_empty());
    display.select(Layer::Background);
    assert_eq!(display.prepare(&scene), vec![Rect::new(0, 0, 280, 192)]);
    assert!(display.pixels().iter().any(|&byte| byte != 0));
    display.acknowledge(&mut scene);
    display.select(Layer::Background);
    assert!(display.prepare(&scene).is_empty());
    display.select(Layer::Foreground);
    assert_eq!(display.prepare(&scene), vec![Rect::new(0, 0, 280, 192)]);
    assert!(display.pixels().iter().all(|&byte| byte == 0));
}

// An adjacent sprite changes the color of ink outside its own cell. The upload
// halo must cover that neighbor without repainting the rest of the scanline.
#[test]
fn artifact_color_updates_cross_the_dirty_cell_boundary() {
    let mut scene = Scene::default();
    let mut display = refreshed(&mut scene);
    let mut original = Hires::default();
    scene.overlay(8, 50, 55);
    draw_sprite_at_pixels(&mut original, 8, 50, 55);
    display.prepare(&scene);
    display.acknowledge(&mut scene);
    let pixel = (60 * WIDTH + 109) * 3;
    assert_eq!(&display.pixels()[pixel..pixel + 3], &[255, 128, 0]);
    scene.overlay(11, 55, 55);
    draw_sprite_at_pixels(&mut original, 11, 55, 55);
    assert_eq!(display.prepare(&scene), vec![Rect::new(109, 55, 12, 11)]);
    assert_eq!(&display.pixels()[pixel..pixel + 3], &[255, 255, 255]);
    for row in 0..HEIGHT {
        assert_eq!(
            &display.pixels()[row * WIDTH * 3..(row + 1) * WIDTH * 3],
            original.rgb_row(row)
        );
    }
}

#[test]
fn right_edge_spill_and_status_updates_keep_local_rgb_deltas() {
    let mut scene = Scene::default();
    let mut display = refreshed(&mut scene);
    let mut original = Hires::default();
    scene.overlay(37, 136, 55);
    draw_sprite_at_pixels(&mut original, 37, 136, 55);
    let regions = display.prepare(&scene);
    assert!(
        regions
            .iter()
            .any(|rect| rect.x() == 0 && rect.contains_point((1, 124)))
    );
    assert!(regions.iter().all(|rect| rect.width() <= 12));
    for row in 0..HEIGHT {
        assert_eq!(
            &display.pixels()[row * WIDTH * 3..(row + 1) * WIDTH * 3],
            original.rgb_row(row)
        );
    }
    display.acknowledge(&mut scene);
    scene.erase(37, 136, 55);
    erase_sprite_at_pixels(&mut original, 37, 136, 55);
    display.prepare(&scene);
    for row in 0..HEIGHT {
        assert_eq!(
            &display.pixels()[row * WIDTH * 3..(row + 1) * WIDTH * 3],
            original.rgb_row(row)
        );
    }
    display.acknowledge(&mut scene);
    scene.tile(Layer::Foreground, 69, 10, 16);
    assert!(
        display
            .prepare(&scene)
            .iter()
            .all(|rect| rect.y() >= 176 && rect.y() + rect.height() as i32 <= 192)
    );
}

// SDL readback checks the actual strided subrectangle uploads. Copy explicitly
// before reading rather than relying on the backbuffer surviving a present.
#[test]
#[ignore = "run through make -C port test-render-sdl with the dummy SDL driver"]
fn sdl_partial_texture_updates_match_the_rgb_cache() {
    let sdl = sdl2::init().unwrap();
    let video = sdl.video().unwrap();
    let window = video
        .window("render test", WIDTH as u32, HEIGHT as u32)
        .hidden()
        .build()
        .unwrap();
    let mut canvas = window.into_canvas().software().build().unwrap();
    let creator = canvas.texture_creator();
    let mut texture = creator
        .create_texture_streaming(PixelFormatEnum::RGB24, WIDTH as u32, HEIGHT as u32)
        .unwrap();
    let mut scene = Scene::default();
    let mut display = Display::default();
    for step in 0..8 {
        match step {
            1 => scene.overlay(8, 50, 55),
            2 => scene.overlay(11, 55, 55),
            3 => scene.erase(8, 50, 55),
            4 => scene.overlay(37, 136, 55),
            5 => {
                scene.tile(Layer::Background, 3, 10, 5);
                display.select(Layer::Background);
            }
            6 => display.select(Layer::Foreground),
            7 => display.request_present(),
            _ => (),
        }
        display
            .paint(&mut scene, &mut canvas, &mut texture)
            .unwrap();
        canvas.copy(&texture, None, None).unwrap();
        assert_eq!(
            canvas.read_pixels(None, PixelFormatEnum::RGB24).unwrap(),
            display.pixels()
        );
    }
}
