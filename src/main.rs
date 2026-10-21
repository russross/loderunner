mod frame_timings;

use crt_native::BackendPreference;
use frame_timings::FrameTimings;
use lode_trace::Phase;
use loderunner::app::{MOVEMENT_STYLE, NEXT_PACK, PREVIOUS_PACK};
use loderunner::cli::{Arguments, CONTROLS, USAGE};
use loderunner::devices::keyboard::{Control, ESCAPE, LevelNavigation, RETURN};
use loderunner::presentation::Presentation;
use loderunner::render::Display;
use loderunner::render::{fit_display, snap_size};
use loderunner::runtime::Session;
use loderunner::timing::NTSC_CPU_HZ;
use loderunner::transition::VisualEffect;
use sdl2::EventPump;
use sdl2::audio::AudioSpecDesired;
use sdl2::event::{Event, WindowEvent};
use sdl2::keyboard::{Keycode, Mod};
use std::cell::{Cell, RefCell};
use std::thread::sleep;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WindowCommand {
    Scale(bool),
    ToggleRenderer,
}

fn window_command(key: Keycode, modifiers: Mod) -> Option<WindowCommand> {
    if modifiers.intersects(Mod::LALTMOD | Mod::RALTMOD | Mod::LGUIMOD | Mod::RGUIMOD) {
        return None;
    }
    if modifiers.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD) {
        return match key {
            Keycode::Equals | Keycode::Plus | Keycode::KpPlus => Some(WindowCommand::Scale(true)),
            Keycode::Minus | Keycode::KpMinus => Some(WindowCommand::Scale(false)),
            _ => None,
        };
    }
    Some(match key {
        Keycode::C => WindowCommand::ToggleRenderer,
        _ => return None,
    })
}

fn key_code(key: Keycode, modifiers: Mod) -> Option<u8> {
    if modifiers.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD) {
        return match key {
            Keycode::R => Some(Control::EndGame as u8),
            Keycode::S => Some(Control::ToggleSound as u8),
            Keycode::Left => Some(Control::Slower as u8),
            Keycode::Right => Some(Control::Faster as u8),
            Keycode::K => Some(MOVEMENT_STYLE),
            Keycode::PageUp => Some(PREVIOUS_PACK),
            Keycode::PageDown => Some(NEXT_PACK),
            _ => None,
        };
    }
    match key {
        Keycode::Left => Some(0xca),
        Keycode::Right => Some(0xcc),
        Keycode::Up => Some(0xc9),
        Keycode::Down => Some(0xcb),
        Keycode::Z => Some(0xd5),
        Keycode::X => Some(0xcf),
        Keycode::Space => Some(0xa0),
        Keycode::P => Some(ESCAPE),
        Keycode::Escape => Some(0x87),
        Keycode::I | Keycode::J | Keycode::K | Keycode::L | Keycode::U | Keycode::O => None,
        Keycode::Return => Some(RETURN),
        Keycode::PageUp => Some(LevelNavigation::Previous as u8),
        Keycode::PageDown => Some(LevelNavigation::Next as u8),
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
            } if !keymod.intersects(
                Mod::LCTRLMOD
                    | Mod::RCTRLMOD
                    | Mod::LALTMOD
                    | Mod::RALTMOD
                    | Mod::LGUIMOD
                    | Mod::RGUIMOD,
            ) =>
            {
                quit.set(true)
            }
            Event::Window { .. } => display.request_present(),
            _ => (),
        }
    }
}

fn display(mut session: Session) -> Result<(), String> {
    let mut timings = FrameTimings::from_env()?;
    let sdl = sdl2::init()?;
    let video = sdl.video()?;
    let [width, height] = session.run.game.host.size;
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
    let mut held_arrows = Vec::new();
    let mut input_context = (session.run.game.session.attempt, false);
    let mut paused_release = false;

    // Initialize the lazy GPU pipeline before starting the live audio timeline.
    presentation.paint_effect(
        &mut session.run.game.scene,
        &mut session.run.game.display,
        VisualEffect::Iris { frame: 0 },
    )?;
    queue.resume();

    // Window commands remain live during playback; gameplay input is selected by the session.
    let result = session.execute(
        |game| {
            if quit_requested.get() {
                return Ok((true, None));
            }
            // Reloads and demo transitions discard host holds. A release cannot
            // dismiss the next ready prompt or become movement after a demo.
            let context = (game.session.attempt, game.demonstrating);
            if input_context != context {
                held_arrows.clear();
                paused_release = false;
                input_context = context;
            }
            if !game.host.commodore {
                paused_release = false;
            }
            let mut delivery = if paused_release && game.session.phase == Phase::Playing {
                paused_release = false;
                held_arrows
                    .last()
                    .and_then(|&held| key_code(held, Mod::NOMOD))
                    .or(Some(0xa0))
            } else {
                None
            };
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
                    } if !keymod.intersects(
                        Mod::LCTRLMOD
                            | Mod::RCTRLMOD
                            | Mod::LALTMOD
                            | Mod::RALTMOD
                            | Mod::LGUIMOD
                            | Mod::RGUIMOD,
                    ) =>
                    {
                        return Ok((true, None));
                    }
                    Event::KeyDown {
                        keycode: Some(key),
                        keymod,
                        repeat,
                        ..
                    } => {
                        if matches!(
                            key,
                            Keycode::Left | Keycode::Right | Keycode::Up | Keycode::Down
                        ) && !keymod.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD)
                        {
                            held_arrows.retain(|&held| held != key);
                            held_arrows.push(key);
                            if game.host.commodore && game.session.phase == Phase::Paused {
                                paused_release = true;
                            }
                        }
                        if let Some(command) = window_command(key, keymod) {
                            if !repeat {
                                commands.borrow_mut().push(command);
                            }
                        } else if !repeat
                            || (game.session.phase != Phase::Waiting
                                && !matches!(key, Keycode::PageUp | Keycode::PageDown))
                        {
                            delivery = key_code(key, keymod);
                        }
                    }
                    Event::KeyUp {
                        keycode: Some(key), ..
                    } => {
                        let active = held_arrows.last() == Some(&key);
                        held_arrows.retain(|&held| held != key);
                        if game.host.commodore
                            && active
                            && !game.demonstrating
                            && game.session.phase != Phase::Waiting
                        {
                            if game.session.phase == Phase::Paused {
                                paused_release = true;
                            } else {
                                delivery = held_arrows
                                    .last()
                                    .and_then(|&held| key_code(held, Mod::NOMOD))
                                    .or(Some(0xa0));
                            }
                        }
                    }
                    _ => (),
                }
            }
            Ok((false, delivery))
        },
        |game| {
            let started = timings.as_ref().map(|_| Instant::now());
            // Window actions are applied at the presentation checkpoint and never
            // enter the gameplay keyboard latch or consumed-input recordings.
            for command in commands.borrow_mut().drain(..) {
                match command {
                    WindowCommand::Scale(larger) => {
                        let (width, height) = snap_size(game.host.size, larger);
                        presentation
                            .window_mut()
                            .set_size(width, height)
                            .map_err(|error| error.to_string())?;
                        game.display.request_present();
                    }
                    WindowCommand::ToggleRenderer => {
                        presentation.toggle(&mut game.display);
                        game.host.renderer = game.host.renderer.toggled();
                    }
                }
            }
            let audio_started = started.map(|_| Instant::now());
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
            let graphics_started = started.map(|_| Instant::now());
            presentation.paint_effect(&mut game.scene, &mut game.display, game.visual_effect)?;
            let (width, height) = presentation.window_mut().size();
            let fitted = fit_display(width, height);
            game.host.size = [fitted.width(), fitted.height()];
            if let (Some(timings), Some(started), Some(audio), Some(graphics)) =
                (&mut timings, started, audio_started, graphics_started)
            {
                timings.record(
                    game.clock.output_cycles(),
                    game.transitioning,
                    started,
                    audio,
                    graphics,
                );
            }

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
    );
    if let Some(timings) = timings {
        timings.finish()?;
    }
    result
}

fn execute() -> Result<(), String> {
    let Some(arguments) = Arguments::parse(std::env::args().skip(1))? else {
        println!("{USAGE}\n\n{CONTROLS}");
        return Ok(());
    };
    let session = Session::new(arguments)?;
    println!(
        "Lode Runner\n\n{CONTROLS}\n\nRenderer: {}\n",
        session.render_mode().name()
    );
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
    use super::{WindowCommand, key_code, window_command};
    use loderunner::devices::keyboard::LevelNavigation;
    use sdl2::keyboard::{Keycode, Mod};

    #[test]
    fn page_keys_select_application_navigation() {
        assert_eq!(
            key_code(Keycode::PageUp, Mod::NOMOD),
            Some(LevelNavigation::Previous as u8)
        );
        assert_eq!(
            key_code(Keycode::PageDown, Mod::NOMOD),
            Some(LevelNavigation::Next as u8)
        );
        assert_eq!(
            key_code(Keycode::PageUp, Mod::LCTRLMOD),
            Some(super::PREVIOUS_PACK)
        );
        assert_eq!(
            key_code(Keycode::PageDown, Mod::LCTRLMOD),
            Some(super::NEXT_PACK)
        );
    }

    #[test]
    fn window_shortcuts_remain_separate_from_modified_game_commands() {
        for (key, scale) in [
            (Keycode::Equals, true),
            (Keycode::Plus, true),
            (Keycode::Minus, false),
        ] {
            assert_eq!(
                window_command(key, Mod::LCTRLMOD),
                Some(WindowCommand::Scale(scale))
            );
            assert_eq!(window_command(key, Mod::NOMOD), None);
        }
        assert_eq!(
            window_command(Keycode::C, Mod::NOMOD),
            Some(WindowCommand::ToggleRenderer)
        );
        assert_eq!(window_command(Keycode::C, Mod::LCTRLMOD), None);
        assert_eq!(window_command(Keycode::F2, Mod::NOMOD), None);
        assert_eq!(window_command(Keycode::Z, Mod::NOMOD), None);
        assert_eq!(window_command(Keycode::Num6, Mod::NOMOD), None);
    }
}
