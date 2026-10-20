use crt_native::BackendPreference;
use lode_trace::Phase;
use loderunner::cli::{Arguments, CONTROLS, USAGE};
use loderunner::devices::keyboard::{Control, ESCAPE, RETURN};
use loderunner::presentation::Presentation;
use loderunner::render::Display;
use loderunner::render::scaled_size;
use loderunner::runtime::Session;
use loderunner::timing::NTSC_CPU_HZ;
use loderunner::transition::VisualEffect;
use sdl2::EventPump;
use sdl2::audio::AudioSpecDesired;
use sdl2::event::{Event, WindowEvent};
use sdl2::keyboard::{Keycode, Mod};
use std::cell::{Cell, RefCell};
use std::thread::sleep;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WindowCommand {
    Scale(u32),
    ToggleRenderer,
}

fn window_command(key: Keycode, modifiers: Mod) -> Option<WindowCommand> {
    if modifiers.intersects(
        Mod::LCTRLMOD | Mod::RCTRLMOD | Mod::LALTMOD | Mod::RALTMOD | Mod::LGUIMOD | Mod::RGUIMOD,
    ) {
        return None;
    }
    Some(match key {
        Keycode::Num1 => WindowCommand::Scale(1),
        Keycode::Num2 => WindowCommand::Scale(2),
        Keycode::Num3 => WindowCommand::Scale(3),
        Keycode::Num4 => WindowCommand::Scale(4),
        Keycode::Num5 => WindowCommand::Scale(5),
        Keycode::Tab => WindowCommand::ToggleRenderer,
        _ => return None,
    })
}

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

// Animation input never dismisses the later ready prompt. Native close requests
// survive draining, and exposure or resize requests refresh the next frame.
fn poll_transition(events: &mut EventPump, quit: &Cell<bool>, display: &mut Display) {
    for event in events.poll_iter() {
        match event {
            Event::Quit { .. } => quit.set(true),
            Event::KeyDown {
                keycode: Some(Keycode::Q),
                keymod,
                ..
            } if keymod.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD) => quit.set(true),
            Event::Window { .. } => display.request_present(),
            _ => (),
        }
    }
}

fn display(mut session: Session) -> Result<(), String> {
    let sdl = sdl2::init()?;
    let video = sdl.video()?;
    let (width, height) = scaled_size(4);
    let window = video
        .window("Lode Runner", width, height)
        .position_centered()
        .resizable()
        .allow_highdpi()
        .metal_view()
        .build()
        .map_err(|error| error.to_string())?;
    let mut presentation = Presentation::new(
        window,
        session.render_mode(),
        BackendPreference::from_env()?,
    )?;
    let commands = RefCell::new(Vec::new());
    let quit_requested = Cell::new(false);
    let events = RefCell::new(sdl.event_pump()?);
    let audio = sdl.audio()?;
    let desired = AudioSpecDesired {
        freq: Some(44100),
        channels: Some(1),
        samples: Some(1024),
    };
    let queue = audio.open_queue::<f32, _>(None, &desired)?;
    let rate = queue.spec().freq as u32;
    let mut sample_frame = 0;
    let mut previous_input = -1.0;
    let mut previous_output = 0.0;

    // Initialize the lazy GPU pipeline before starting the live audio timeline.
    presentation.paint_effect(
        &mut session.run.game.scene,
        &mut session.run.game.display,
        VisualEffect::Iris { frame: 0 },
    )?;
    queue.resume();

    // Window commands remain live during playback; gameplay input is selected by the session.
    session.execute(
        |game| {
            if quit_requested.get() {
                return Ok((true, None));
            }
            let mut delivery = None;
            for event in events.borrow_mut().poll_iter() {
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
                        repeat,
                        ..
                    } => {
                        if let Some(command) = window_command(key, keymod) {
                            if !repeat {
                                commands.borrow_mut().push(command);
                            }
                        } else if !repeat || game.session.phase != Phase::Waiting {
                            delivery = key_code(key, keymod);
                        }
                    }
                    _ => (),
                }
            }
            Ok((false, delivery))
        },
        |game| {
            // Window actions are applied at the presentation checkpoint and never
            // enter the gameplay keyboard latch or consumed-input recordings.
            for command in commands.borrow_mut().drain(..) {
                match command {
                    WindowCommand::Scale(scale) => {
                        let (width, height) = scaled_size(scale);
                        presentation
                            .window_mut()
                            .set_size(width, height)
                            .map_err(|error| error.to_string())?;
                        game.display.request_present();
                    }
                    WindowCommand::ToggleRenderer => presentation.toggle(&mut game.display),
                }
            }
            let final_frame = (u128::from(game.clock.output_cycles()) * u128::from(rate)
                / u128::from(NTSC_CPU_HZ)) as u64;
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
            presentation.paint_effect(&mut game.scene, &mut game.display, game.visual_effect)?;

            // Transition tunes can span seconds. Pump native window events while
            // waiting, retaining queued keys for the next normal input boundary.
            while let Some(remaining) = game.clock.remaining() {
                if remaining.is_zero() {
                    break;
                }
                events.borrow_mut().pump_events();
                if game.transitioning {
                    poll_transition(&mut events.borrow_mut(), &quit_requested, &mut game.display);
                }
                sleep(remaining.min(Duration::from_millis(5)));
            }
            if game.transitioning {
                poll_transition(&mut events.borrow_mut(), &quit_requested, &mut game.display);
            }
            Ok(())
        },
    )
}

fn execute() -> Result<(), String> {
    let Some(arguments) = Arguments::parse(std::env::args().skip(1))? else {
        println!("{USAGE}\n\n{CONTROLS}");
        return Ok(());
    };
    println!(
        "Lode Runner\n\n{CONTROLS}\n\nRenderer: {}\n",
        arguments.renderer.name()
    );
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

#[cfg(test)]
mod tests {
    use super::{WindowCommand, window_command};
    use sdl2::keyboard::{Keycode, Mod};

    #[test]
    fn window_shortcuts_remain_separate_from_modified_game_commands() {
        for (key, scale) in [
            (Keycode::Num1, 1),
            (Keycode::Num2, 2),
            (Keycode::Num3, 3),
            (Keycode::Num4, 4),
            (Keycode::Num5, 5),
        ] {
            assert_eq!(
                window_command(key, Mod::NOMOD),
                Some(WindowCommand::Scale(scale))
            );
            assert_eq!(window_command(key, Mod::LCTRLMOD), None);
        }
        assert_eq!(
            window_command(Keycode::Tab, Mod::NOMOD),
            Some(WindowCommand::ToggleRenderer)
        );
        assert_eq!(window_command(Keycode::Tab, Mod::LCTRLMOD), None);
        assert_eq!(window_command(Keycode::F2, Mod::NOMOD), None);
        assert_eq!(window_command(Keycode::Z, Mod::NOMOD), None);
        assert_eq!(window_command(Keycode::Num6, Mod::NOMOD), None);
    }
}
