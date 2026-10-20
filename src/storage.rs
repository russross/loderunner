use crate::assets::LEVEL_RECORDS;
use crate::board::Level;
use lode_trace::Tile;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LevelRecord(pub [u8; 256]);

impl LevelRecord {
    pub fn decode(&self) -> Result<Level, String> {
        Level::new(decode_level(&self.0))
    }
}

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

pub fn original_level(number: u8) -> Result<[u8; 256], String> {
    if !(1..=150).contains(&number) {
        return Err("level must be between 1 and 150".into());
    }
    Ok(LEVEL_RECORDS[usize::from(number - 1)])
}

// Disk records contain two cells per byte, low nibble first. Bytes 224 through
// 255 are not playfield cells and remain available to storage round trips.
pub fn decode_level(record: &[u8; 256]) -> [Tile; 448] {
    std::array::from_fn(|index| {
        let byte = record[index / 2];
        Tile::decode(if index % 2 == 0 { byte & 15 } else { byte >> 4 })
    })
}
