use lode_mame_validation::{Options, validate};
use lode_trace::Trace;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() != Some("validate") {
        return Err("usage: lode-mame-validation validate TRACE [--work DIR] [--mame PATH] [--rompath PATH] [--tolerance-cycles N] [--tolerance-percent N] [--transition-tolerance-cycles N] [--timeout-seconds N] [--original-initialization]".into());
    }
    let path = PathBuf::from(args.next().ok_or("validate requires a trace file")?);
    let mut options = Options::default();
    while let Some(option) = args.next() {
        if option == "--original-initialization" {
            options.fixed_initialization = false;
            continue;
        }
        let value = args
            .next()
            .ok_or_else(|| format!("{option} requires a value"))?;
        match option.as_str() {
            "--work" => options.work = Some(PathBuf::from(value)),
            "--mame" => options.mame = value.into(),
            "--rompath" => options.rompath = Some(value),
            "--tolerance-cycles" => {
                options.gameplay.cycles = value.parse().map_err(|_| "invalid cycle tolerance")?
            }
            "--tolerance-percent" => {
                options.gameplay.percent =
                    value.parse().map_err(|_| "invalid percentage tolerance")?
            }
            "--transition-tolerance-cycles" => {
                options.transitions.cycles =
                    value.parse().map_err(|_| "invalid transition tolerance")?
            }
            "--timeout-seconds" => {
                options.timeout = Duration::from_secs(value.parse().map_err(|_| "invalid timeout")?)
            }
            _ => return Err(format!("unknown option {option}")),
        }
    }
    let text = fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    let trace = Trace::from_json(&text).map_err(|error| error.to_string())?;
    let report = validate(&trace, &options)?;
    println!(
        "Validated {} intervals against {}. Report: {}",
        report.intervals,
        report.machine,
        report.work.join("report.json").display()
    );
    Ok(())
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("lode-mame-validation: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
