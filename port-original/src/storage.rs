use lode_trace::Tile;

const TRACKS: [&str; 10] = [
    include_str!("../../sources/lode_runner_reveng/disk/track_03.asm"),
    include_str!("../../sources/lode_runner_reveng/disk/track_04.asm"),
    include_str!("../../sources/lode_runner_reveng/disk/track_05.asm"),
    include_str!("../../sources/lode_runner_reveng/disk/track_06.asm"),
    include_str!("../../sources/lode_runner_reveng/disk/track_07.asm"),
    include_str!("../../sources/lode_runner_reveng/disk/track_08.asm"),
    include_str!("../../sources/lode_runner_reveng/disk/track_09.asm"),
    include_str!("../../sources/lode_runner_reveng/disk/track_0a.asm"),
    include_str!("../../sources/lode_runner_reveng/disk/track_0b.asm"),
    include_str!("../../sources/lode_runner_reveng/disk/track_0c.asm"),
];

pub fn original_level(number: u8) -> Result<[u8; 256], String> {
    if !(1..=150).contains(&number) {
        return Err("level must be between 1 and 150".into());
    }
    let index = usize::from(number - 1);
    let mut found = false;
    let mut bytes = Vec::new();
    for line in TRACKS[index / 16].lines() {
        if let Some((_, sector)) = line.split_once(", sector $") {
            if found {
                break;
            }
            found = usize::from_str_radix(sector.trim(), 16).map_err(|error| error.to_string())?
                == index % 16;
        } else if found && let Some(hex) = line.trim().strip_prefix("HEX") {
            for byte in hex.split_whitespace() {
                bytes.push(u8::from_str_radix(byte, 16).map_err(|error| error.to_string())?);
            }
        }
    }
    bytes.try_into().map_err(|bytes: Vec<u8>| {
        format!("level {number}: expected 256 bytes, found {}", bytes.len())
    })
}

// Disk records contain two cells per byte, low nibble first. Bytes 224 through
// 255 are not playfield cells and remain available to storage round trips.
pub fn decode_level(record: &[u8; 256]) -> [Tile; 448] {
    std::array::from_fn(|index| {
        let byte = record[index / 2];
        Tile::decode(if index % 2 == 0 { byte & 15 } else { byte >> 4 })
    })
}
