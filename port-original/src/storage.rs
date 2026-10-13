use lode_trace::Tile;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LevelRecord(pub [u8; 256]);

impl Serialize for LevelRecord {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(&self.0)
    }
}

impl<'de> Deserialize<'de> for LevelRecord {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let bytes = Vec::<u8>::deserialize(deserializer)?;
        let record = bytes
            .try_into()
            .map_err(|_| serde::de::Error::custom("level record must contain 256 bytes"))?;
        Ok(Self(record))
    }
}

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
    read_sector(index / 16, index % 16)
}

fn read_sector(track: usize, wanted_sector: usize) -> Result<[u8; 256], String> {
    let mut found = false;
    let mut bytes = Vec::new();
    for line in TRACKS[track].lines() {
        if let Some((_, sector)) = line.split_once(", sector $") {
            if found {
                break;
            }
            found = usize::from_str_radix(sector.trim(), 16).map_err(|error| error.to_string())?
                == wanted_sector;
        } else if found && let Some(hex) = line.trim().strip_prefix("HEX") {
            for byte in hex.split_whitespace() {
                bytes.push(u8::from_str_radix(byte, 16).map_err(|error| error.to_string())?);
            }
        }
    }
    bytes.try_into().map_err(|bytes: Vec<u8>| {
        format!(
            "track {track} sector {wanted_sector}: expected 256 bytes, found {}",
            bytes.len()
        )
    })
}

pub fn original_high_scores() -> Result<Vec<lode_trace::HighScore>, String> {
    let sector = read_sector(9, 15)?;
    Ok((0..10)
        .map(|index| {
            let row = &sector[index * 8..index * 8 + 8];
            let score = row[4..8].iter().fold(0, |score, pair| {
                score * 100 + u32::from(pair >> 4) * 10 + u32::from(pair & 15)
            });
            lode_trace::HighScore {
                initials: [row[0] & 0x7f, row[1] & 0x7f, row[2] & 0x7f],
                level: row[3],
                score,
            }
        })
        .collect())
}

// Disk records contain two cells per byte, low nibble first. Bytes 224 through
// 255 are not playfield cells and remain available to storage round trips.
pub fn decode_level(record: &[u8; 256]) -> [Tile; 448] {
    std::array::from_fn(|index| {
        let byte = record[index / 2];
        Tile::decode(if index % 2 == 0 { byte & 15 } else { byte >> 4 })
    })
}
