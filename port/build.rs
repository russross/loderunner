//! Convert pinned reference assets into native data before compiling the port.
use std::env;
use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn read(path: &Path) -> Result<Vec<u8>, String> {
    println!("cargo:rerun-if-changed={}", path.display());
    fs::read(path).map_err(|error| format!("{}: {error}", path.display()))
}

// Disk records remain byte-exact for trace configuration and machine validation.
// The text parser is confined to the reference conversion performed at build time.
fn sector(track: &str, wanted: usize) -> Result<[u8; 256], String> {
    let mut found = false;
    let mut bytes = Vec::new();
    for line in track.lines() {
        if let Some((_, number)) = line.split_once(", sector $") {
            if found {
                break;
            }
            found = usize::from_str_radix(number.trim(), 16).map_err(|error| error.to_string())?
                == wanted;
        } else if found && let Some(hex) = line.trim().strip_prefix("HEX") {
            for byte in hex.split_whitespace() {
                bytes.push(u8::from_str_radix(byte, 16).map_err(|error| error.to_string())?);
            }
        }
    }
    bytes.try_into().map_err(|bytes: Vec<u8>| {
        format!("sector {wanted}: expected 256 bytes, found {}", bytes.len())
    })
}

fn generate() -> Result<(), String> {
    println!("cargo:rerun-if-changed=build.rs");
    let source = Path::new("../sources/lode_runner_reveng");
    let image = read(&source.join("main.bin"))?;
    let byte = |address: usize| {
        image
            .get(address)
            .copied()
            .ok_or_else(|| format!("sprite data address {address:04x} exceeds reference image"))
    };
    let patterns = usize::from(byte(0x84c1)?) << 8;
    let mut output = String::from(
        "// Generated from the pinned reference; edit build.rs or the reference inputs.\n",
    );
    output.push_str("pub const SPRITE_ROWS: [[u16; 11]; 102] = [\n");

    // Original sprite identities remain stable for timing calculations. Each row
    // becomes ten native ink bits, independent of machine pointers and shifts.
    for sprite in 0..102 {
        let mut rows = [0_u16; 11];
        for (row, pixels) in rows.iter_mut().enumerate() {
            for plane in 0..2 {
                let pattern = usize::from(byte(0xad00 + (row * 2 + plane) * 104 + sprite)?);
                let address = usize::from(byte(patterns + pattern)?)
                    | usize::from(byte(patterns + 0x80 + pattern)?) << 8;
                let ink =
                    u16::from(byte(address)? & 0x7f) | u16::from(byte(address + 1)? & 0x7f) << 7;
                *pixels |= ink << (plane * 7);
            }
            if *pixels >= 1024 {
                return Err(format!("sprite {sprite} row {row} exceeds ten pixels"));
            }
        }
        writeln!(output, "{rows:?},").expect("writing generated string");
    }
    output.push_str("];\npub static LEVEL_RECORDS: [[u8; 256]; 150] = [\n");

    let mut tracks = Vec::new();
    for number in 3..=12 {
        let path = source.join(format!("disk/track_{number:02x}.asm"));
        tracks.push(String::from_utf8(read(&path)?).map_err(|error| error.to_string())?);
    }
    for index in 0..150 {
        let record = sector(&tracks[index / 16], index % 16)
            .map_err(|error| format!("level {}: {error}", index + 1))?;
        writeln!(output, "{record:?},").expect("writing generated string");
    }
    output.push_str("];\npub const HIGH_SCORES: [HighScore; 10] = [\n");

    // Decimal scores and plain initials are runtime data. Packed BCD and the
    // high-bit character convention end at this reference conversion boundary.
    let scores = sector(&tracks[9], 15)?;
    for row in scores[..80].as_chunks::<8>().0 {
        let initials = [row[0] & 0x7f, row[1] & 0x7f, row[2] & 0x7f];
        let level = row[3];
        let score = row[4..8].iter().fold(0_u32, |score, pair| {
            score * 100 + u32::from(pair >> 4) * 10 + u32::from(pair & 15)
        });
        writeln!(
            output,
            "HighScore {{ initials: {initials:?}, level: {level}, score: {score} }},"
        )
        .expect("writing generated string");
    }
    output.push_str("];\n");
    let directory = PathBuf::from(env::var_os("OUT_DIR").ok_or("Cargo did not supply OUT_DIR")?);
    fs::write(directory.join("assets.rs"), output).map_err(|error| error.to_string())
}

fn main() -> ExitCode {
    match generate() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("reference asset conversion: {error}");
            ExitCode::FAILURE
        }
    }
}
