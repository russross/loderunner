use lode_port::render::{HEIGHT, WIDTH};
use lode_port::session::{Configuration, Run, replay};
use lode_port::timing::{Clock, NTSC_CPU_HZ};
use lode_trace::Trace;
use sdl2::audio::AudioSpecDesired;
use sdl2::event::{Event, WindowEvent};
use sdl2::keyboard::{Keycode, Mod};
use sdl2::pixels::PixelFormatEnum;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

enum Command {
    Preview(u8),
    Play {
        level: u8,
        trace: Option<PathBuf>,
        rounds: Option<usize>,
    },
    Capture {
        level: u8,
        path: PathBuf,
        rounds: usize,
        inputs: Vec<(usize, u8)>,
    },
    Replay(PathBuf),
}

const USAGE: &str = "usage:\n  lode-port play [--level N] [--trace FILE] [--rounds N]\n  lode-port capture FILE [--level N] --rounds N [--input ROUND:HEXCODE]...\n  lode-port replay FILE\n  lode-port preview [N]";

fn parse() -> Result<Command, String> {
    let mut args = std::env::args().skip(1);
    let command = args.next().ok_or(USAGE)?;
    if command == "preview" {
        let level = args.next().map_or(Ok(1), |value| {
            value.parse::<u8>().map_err(|_| "invalid level".to_owned())
        })?;
        if args.next().is_some() {
            return Err(USAGE.into());
        }
        return Ok(Command::Preview(level));
    }
    if command == "replay" {
        let path = PathBuf::from(args.next().ok_or("replay requires a trace file")?);
        if args.next().is_some() {
            return Err(USAGE.into());
        }
        return Ok(Command::Replay(path));
    }
    if command != "play" && command != "capture" {
        return Err(USAGE.into());
    }
    let path = if command == "capture" {
        Some(PathBuf::from(
            args.next().ok_or("capture requires an output file")?,
        ))
    } else {
        None
    };
    let mut level = 1;
    let mut trace = None;
    let mut rounds = None;
    let mut inputs = Vec::new();
    while let Some(option) = args.next() {
        let value = args
            .next()
            .ok_or_else(|| format!("{option} requires a value"))?;
        match option.as_str() {
            "--level" => level = value.parse::<u8>().map_err(|_| "invalid level")?,
            "--trace" if command == "play" => trace = Some(PathBuf::from(value)),
            "--rounds" => rounds = Some(value.parse::<usize>().map_err(|_| "invalid round count")?),
            "--input" if command == "capture" => {
                let (round, key) = value.split_once(':').ok_or("input must be ROUND:HEXCODE")?;
                inputs.push((
                    round.parse::<usize>().map_err(|_| "invalid input round")?,
                    u8::from_str_radix(key, 16).map_err(|_| "invalid input code")?,
                ));
            }
            _ => return Err(format!("unknown option {option}")),
        }
    }
    if let Some(path) = path {
        Ok(Command::Capture {
            level,
            path,
            rounds: rounds.ok_or("capture requires --rounds")?,
            inputs,
        })
    } else {
        Ok(Command::Play {
            level,
            trace,
            rounds,
        })
    }
}

fn save_trace(path: &PathBuf, trace: &Trace) -> Result<(), String> {
    let text = trace.to_json().map_err(|error| error.to_string())?;
    fs::write(path, text).map_err(|error| format!("{}: {error}", path.display()))
}

fn key_code(key: Keycode, modifiers: Mod) -> Option<u8> {
    if modifiers.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD) {
        return match key {
            Keycode::A => Some(0x81),
            Keycode::R => Some(0x92),
            Keycode::S => Some(0x93),
            Keycode::Left => Some(0x88),
            Keycode::Right => Some(0x95),
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
        Keycode::Escape => Some(0x9b),
        Keycode::Return => Some(0x8d),
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

fn display(
    level: u8,
    preview: bool,
    trace_path: Option<PathBuf>,
    limit: Option<usize>,
) -> Result<(), String> {
    let mut configuration = Configuration::original(level)?;
    let scores_path = PathBuf::from("highscores.json");
    match fs::read_to_string(&scores_path) {
        Ok(text) => {
            configuration.high_scores =
                serde_json::from_str(&text).map_err(|error| format!("highscores.json: {error}"))?
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
        Err(error) => return Err(format!("highscores.json: {error}")),
    }
    let initial_scores = configuration.high_scores.clone();
    let mut run = Run::new(configuration, false)?;
    if trace_path.is_none() {
        run.disable_tracing()?;
    }
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
    run.game.clock = Clock::new(!preview);
    let mut round = 0;

    // Events feed the original latch. Space supplies the stop code; release events
    // leave persistent keyboard commands intact. Audio is submitted before pacing.
    'play: loop {
        let mut delivery = None;
        for event in events.poll_iter() {
            match event {
                Event::Window {
                    win_event:
                        WindowEvent::Exposed | WindowEvent::Resized(..) | WindowEvent::SizeChanged(..),
                    ..
                } => run.game.display.request_present(),
                Event::Quit { .. } => break 'play,
                Event::KeyDown {
                    keycode: Some(Keycode::Q),
                    keymod,
                    ..
                } if keymod.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD) => break 'play,
                Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    ..
                } if preview => break 'play,
                Event::KeyDown {
                    keycode: Some(key),
                    keymod,
                    ..
                } => delivery = key_code(key, keymod),
                _ => (),
            }
        }
        if !preview {
            run.step_with_checkpoint(delivery, &mut |game| {
                let final_frame = (u128::from(game.clock.cycles) * u128::from(rate)
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
                game.display
                    .paint(&mut game.scene, &mut canvas, &mut texture)?;
                Ok(())
            })?;
        } else {
            run.game
                .display
                .paint(&mut run.game.scene, &mut canvas, &mut texture)?;
        }
        round += 1;
        if limit.is_some_and(|limit| round >= limit) {
            break;
        }
        if preview {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    if let Some(path) = trace_path {
        save_trace(&path, &run.trace()?)?;
    }
    if run.game.high_scores != initial_scores {
        let text = serde_json::to_string_pretty(&run.game.high_scores)
            .map_err(|error| error.to_string())?;
        fs::write(scores_path, text).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn execute(command: Command) -> Result<(), String> {
    match command {
        Command::Preview(level) => display(level, true, None, None),
        Command::Play {
            level,
            trace,
            rounds,
        } => display(level, false, trace, rounds),
        Command::Replay(path) => {
            let text = fs::read_to_string(&path)
                .map_err(|error| format!("{}: {error}", path.display()))?;
            let trace = Trace::from_json(&text).map_err(|error| error.to_string())?;
            replay(&trace)?;
            println!("Validated {} rounds", trace.rounds.len());
            Ok(())
        }
        Command::Capture {
            level,
            path,
            rounds,
            inputs,
        } => {
            let mut configuration = Configuration::original(level)?;
            configuration.wait_on_level_start = false;
            let mut deliveries = vec![None; rounds];
            for (round, key) in inputs {
                let slot = deliveries
                    .get_mut(round)
                    .ok_or("input round exceeds capture length")?;
                if slot.is_some() {
                    return Err("multiple inputs assigned to one round".into());
                }
                *slot = Some(key);
            }
            let mut run = Run::new(configuration, false)?;
            for delivery in deliveries {
                run.step(delivery)?;
            }
            let trace = run.trace()?;
            save_trace(&path, &trace)?;
            println!("Captured {} rounds", trace.rounds.len());
            Ok(())
        }
    }
}

fn main() -> std::process::ExitCode {
    match parse().and_then(execute) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("lode-port: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
