//! One startup contract for live play, input playback, and trace verification.

use crate::presentation::RenderMode;
use lode_mame_validation::Options;
use std::path::PathBuf;
use std::time::Duration;

pub const USAGE: &str = "Lode Runner\n\nUsage: loderunner [OPTIONS]\n  --record FILE       Save effective gameplay inputs\n  --replay FILE       Play an input recording\n  --trace FILE        Save a trace and validate against MAME\n  --verify FILE       Verify a trace against the game and MAME\n  --validate          Validate against MAME without a trace destination\n  --headless          Silent, full speed execution\n  --display           Display and sound (default except for --verify)\n  --unpaced           Disable display pacing\n  --renderer MODE    crt (default) or simple output\n  --rounds N          Bound execution (required for headless live play)\n  --level N           Starting level, 1 through 150\n  --input ROUND:HEX   Schedule a key for development\n  --mame PATH --rompath PATH --pluginspath DIR --work DIR --timeout-seconds N\n  --tolerance-cycles N --tolerance-percent N\n  --original-initialization";

pub const CONTROLS: &str = "Game controls                         Window & sound
  arrows / <space>  move / stop         <ctrl> = / -    resize
  z / x             dig left / right    c               toggle CRT mode
  p                 pause / resume      <ctrl> s        toggle sound
  <ctrl> r          restart level       q               quit
  <ctrl> ← / →      slower / faster
  <ctrl> k          toggle hold-to-move

Levels & demo mode
  PgUp / PgDown       previous / next level
  <ctrl> PgUp/PgDown  previous / next level pack
  d                   start demo (at level start)
  ← / →               previous / next demo (in demo mode)
  <ctrl> ← / →        demo slower / faster (separate from play speed)
  <esc> / <space>     leave demo and return to ready

Ready: press a play key to start.  --help: command-line options.";

#[derive(Default)]
pub enum Playback {
    #[default]
    Live,
    Recording(PathBuf),
    Trace(PathBuf),
}

pub struct Arguments {
    pub playback: Playback,
    pub record: Option<PathBuf>,
    pub trace: Option<PathBuf>,
    pub validate: bool,
    pub display: bool,
    pub paced: bool,
    pub renderer: RenderMode,
    pub rounds: Option<usize>,
    pub level: u8,
    pub level_explicit: bool,
    pub renderer_explicit: bool,
    pub inputs: Vec<(usize, u8)>,
    pub mame: Options,
}

impl Arguments {
    pub fn parse(args: impl Iterator<Item = String>) -> Result<Option<Self>, String> {
        let mut result = Self {
            playback: Playback::Live,
            record: None,
            trace: None,
            validate: false,
            display: true,
            paced: true,
            renderer: RenderMode::Crt,
            rounds: None,
            level: 1,
            level_explicit: false,
            renderer_explicit: false,
            inputs: Vec::new(),
            mame: Options::default(),
        };
        let mut display = None;
        let mut level = false;
        let mut mame_options = false;
        let mut args = args.peekable();
        while let Some(option) = args.next() {
            match option.as_str() {
                "--help" | "-h" => return Ok(None),
                "--validate" => {
                    result.validate = true;
                    continue;
                }
                "--headless" | "--display" => {
                    let selected = option == "--display";
                    if display.is_some_and(|previous| previous != selected) {
                        return Err("--display and --headless are mutually exclusive".into());
                    }
                    display = Some(selected);
                    continue;
                }
                "--unpaced" => {
                    result.paced = false;
                    continue;
                }
                "--original-initialization" => {
                    result.mame.fixed_initialization = false;
                    mame_options = true;
                    continue;
                }
                _ => (),
            }
            let value = args
                .next()
                .ok_or_else(|| format!("{option} requires a value"))?;
            match option.as_str() {
                "--replay" | "--verify" => {
                    if !matches!(result.playback, Playback::Live) {
                        return Err("choose one of --replay and --verify".into());
                    }
                    result.playback = if option == "--replay" {
                        Playback::Recording(value.into())
                    } else {
                        Playback::Trace(value.into())
                    };
                }
                "--renderer" => {
                    result.renderer_explicit = true;
                    result.renderer = match value.as_str() {
                        "crt" => RenderMode::Crt,
                        "simple" => RenderMode::Simple,
                        _ => return Err("renderer must be crt or simple".into()),
                    };
                }
                "--record" => result.record = Some(value.into()),
                "--trace" => result.trace = Some(value.into()),
                "--rounds" => {
                    result.rounds = Some(value.parse().map_err(|_| "invalid round count")?)
                }
                "--level" => {
                    result.level_explicit = true;
                    result.level = value.parse().map_err(|_| "invalid level")?;
                    level = true;
                }
                "--input" => {
                    let (round, key) = value.split_once(':').ok_or("input must be ROUND:HEX")?;
                    let key = u8::from_str_radix(key, 16).map_err(|_| "invalid input code")?;
                    if key < 0x80 {
                        return Err("input codes must have the high bit set".into());
                    }
                    result
                        .inputs
                        .push((round.parse().map_err(|_| "invalid input round")?, key));
                }
                "--mame" => {
                    result.mame.mame = value.into();
                    mame_options = true;
                }
                "--rompath" => {
                    result.mame.rompath = Some(value);
                    mame_options = true;
                }
                "--pluginspath" => {
                    result.mame.pluginspath = Some(value.into());
                    mame_options = true;
                }
                "--work" => {
                    result.mame.work = Some(value.into());
                    mame_options = true;
                }
                "--timeout-seconds" => {
                    result.mame.timeout =
                        Duration::from_secs(value.parse().map_err(|_| "invalid timeout")?);
                    mame_options = true;
                }
                "--tolerance-cycles" => {
                    result.mame.gameplay.cycles =
                        value.parse().map_err(|_| "invalid cycle tolerance")?;
                    mame_options = true;
                }
                "--tolerance-percent" => {
                    result.mame.gameplay.percent =
                        value.parse().map_err(|_| "invalid percentage tolerance")?;
                    mame_options = true;
                }
                _ => return Err(format!("unknown option {option}; use --help")),
            }
        }

        // Playback embeds its starting level configuration.
        let live = matches!(result.playback, Playback::Live);
        if !live && (level || !result.inputs.is_empty()) {
            return Err("playback cannot override --level or --input".into());
        }
        if !(1..=150).contains(&result.level) {
            return Err("level must be between 1 and 150".into());
        }
        if result.rounds == Some(0) {
            return Err("--rounds must be positive".into());
        }
        result.display = display.unwrap_or(!matches!(result.playback, Playback::Trace(_)));
        if live && !result.display && result.rounds.is_none() {
            return Err("headless live play requires --rounds".into());
        }
        result.validate |= result.trace.is_some() || matches!(result.playback, Playback::Trace(_));
        if mame_options && !result.validate {
            return Err("MAME options require --validate, --trace, or --verify".into());
        }
        result.inputs.sort_unstable_by_key(|&(round, _)| round);
        if result.inputs.windows(2).any(|pair| pair[0].0 == pair[1].0) {
            return Err("multiple inputs assigned to one round".into());
        }
        if result
            .inputs
            .iter()
            .any(|&(round, _)| result.rounds.is_some_and(|end| round >= end))
        {
            return Err("input round exceeds run length".into());
        }
        Ok(Some(result))
    }
}
