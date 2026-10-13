use lode_original::devices::hires::{HEIGHT, WIDTH};
use lode_original::state::Game;
use lode_original::storage::original_level;
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::PixelFormatEnum;
use std::time::Duration;

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) != Some("preview") || args.len() > 2 {
        return Err("usage: lode-original preview [level 1..150]".into());
    }
    let level = args.get(1).map_or(Ok(1), |value| {
        value
            .parse::<u8>()
            .map_err(|_| "level must be between 1 and 150".to_owned())
    })?;
    let mut game = Game::load_level(&original_level(level)?, level, false)?;
    let sdl = sdl2::init()?;
    let video = sdl.video()?;
    let window = video
        .window(
            &format!("Lode Runner level {level} preview"),
            WIDTH as u32 * 3,
            HEIGHT as u32 * 3,
        )
        .position_centered()
        .build()
        .map_err(|error| error.to_string())?;
    let mut canvas = window
        .into_canvas()
        .build()
        .map_err(|error| error.to_string())?;
    let creator = canvas.texture_creator();
    let mut texture = creator
        .create_texture_streaming(PixelFormatEnum::RGB24, WIDTH as u32, HEIGHT as u32)
        .map_err(|error| error.to_string())?;
    let mut events = sdl.event_pump()?;

    // The preview exercises the real device paint path. It has no gameplay driver
    // and exits on Escape or a window-close event.
    game.hires.paint(&mut canvas, &mut texture)?;
    loop {
        for event in events.poll_iter() {
            match event {
                Event::Quit { .. }
                | Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    ..
                } => return Ok(()),
                Event::Window { .. } => game.hires.paint(&mut canvas, &mut texture)?,
                _ => (),
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("lode-original: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
