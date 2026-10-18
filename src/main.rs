use loderunner::cli::{Arguments, USAGE};
use loderunner::devices::keyboard::{Control, ESCAPE, RETURN};
use loderunner::render::{HEIGHT, WIDTH};
use loderunner::runtime::Session;
use loderunner::timing::NTSC_CPU_HZ;
use sdl2::audio::AudioSpecDesired;
use sdl2::event::{Event, WindowEvent};
use sdl2::keyboard::{Keycode, Mod};
use sdl2::pixels::PixelFormatEnum;

fn key_code(key: Keycode, modifiers: Mod) -> Option<u8> {
    if modifiers.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD) {
        return match key {
            Keycode::A => Some(Control::Sacrifice as u8),
            Keycode::R => Some(Control::EndGame as u8),
            Keycode::S => Some(Control::ToggleSound as u8),
            Keycode::Left => Some(Control::Slower as u8),
            Keycode::Right => Some(Control::Faster as u8),
            _ => None,
        };
    }
    match key {
        Keycode::Left | Keycode::J => Some(0xca),
        Keycode::Right | Keycode::L => Some(0xcc),
        Keycode::Up | Keycode::I => Some(0xc9),
        Keycode::Down | Keycode::K => Some(0xcb),
        Keycode::Z | Keycode::U => Some(0xd5),
        Keycode::X | Keycode::O => Some(0xcf),
        Keycode::Space => Some(0xa0),
        Keycode::Escape => Some(ESCAPE),
        Keycode::Return => Some(RETURN),
        _ => {
            let code = i32::from(key);
            if (32..=126).contains(&code) {
                Some((code as u8).to_ascii_uppercase() | 0x80)
            } else {
                None
            }
        }
    }
}

fn display(session: Session) -> Result<(), String> {
    let sdl = sdl2::init()?;
    let video = sdl.video()?;
    let window = video
        .window("Lode Runner", WIDTH as u32 * 3, HEIGHT as u32 * 3)
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
    let audio = sdl.audio()?;
    let desired = AudioSpecDesired {
        freq: Some(44100),
        channels: Some(1),
        samples: Some(1024),
    };
    let queue = audio.open_queue::<f32, _>(None, &desired)?;
    queue.queue_audio(&vec![0.0; 1024])?;
    queue.resume();
    let rate = queue.spec().freq as u32;
    let mut sample_frame = 0;
    let mut previous_input = -1.0;
    let mut previous_output = 0.0;

    // Window commands remain live during playback; gameplay input is selected by the session.
    session.execute(
        |game| {
            let mut delivery = None;
            for event in events.poll_iter() {
                match event {
                    Event::Window {
                        win_event:
                            WindowEvent::Exposed
                            | WindowEvent::Resized(..)
                            | WindowEvent::SizeChanged(..),
                        ..
                    } => game.display.request_present(),
                    Event::Quit { .. } => return Ok((true, None)),
                    Event::KeyDown {
                        keycode: Some(Keycode::Q),
                        keymod,
                        ..
                    } if keymod.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD) => {
                        return Ok((true, None));
                    }
                    Event::KeyDown {
                        keycode: Some(key),
                        keymod,
                        ..
                    } => delivery = key_code(key, keymod),
                    _ => (),
                }
            }
            Ok((false, delivery))
        },
        |game| {
            let final_frame =
                (u128::from(game.clock.cycles) * u128::from(rate) / u128::from(NTSC_CPU_HZ)) as u64;
            let mut samples = game.speaker.sample_frames(
                sample_frame,
                (final_frame - sample_frame) as usize,
                rate,
                NTSC_CPU_HZ,
            );
            for sample in &mut samples {
                let output = *sample - previous_input + 0.995 * previous_output;
                previous_input = *sample;
                previous_output = output;
                *sample = output * 0.12;
            }
            queue.queue_audio(&samples)?;
            sample_frame = final_frame;
            game.speaker
                .discard_before(sample_frame * NTSC_CPU_HZ / u64::from(rate));
            game.display
                .paint(&mut game.scene, &mut canvas, &mut texture)
        },
    )
}

fn execute() -> Result<(), String> {
    let Some(arguments) = Arguments::parse(std::env::args().skip(1))? else {
        println!("{USAGE}");
        return Ok(());
    };
    let session = Session::new(arguments)?;
    if session.displayed() {
        display(session)
    } else {
        session.execute(|_| Ok((false, None)), |_| Ok(()))
    }
}

fn main() -> std::process::ExitCode {
    match execute() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("loderunner: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
