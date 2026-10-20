//! Application animation timing is independent of the gameplay cycle model.
use crate::collision::Scene;
use crate::graphics::Sprite;

pub const IRIS_FRAMES: u64 = 30;
pub const IRIS_DURATION_CYCLES: u64 = crate::timing::NTSC_CPU_HZ / 2;
pub const BLINK_HALF_PERIOD_CYCLES: u64 = crate::timing::NTSC_CPU_HZ / 3;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VisualEffect {
    #[default]
    None,
    Iris {
        frame: u64,
    },
    HidePlayer {
        sprite: Sprite,
        half_x: u8,
        y: u8,
    },
}

impl VisualEffect {
    // Effects operate on a presentation copy, preserving gameplay contact ink.
    // The status strip remains visible throughout the centered circular wipe.
    pub fn apply(self, scene: &mut Scene) {
        match self {
            Self::None => (),
            Self::Iris { frame } => scene.iris(frame.min(IRIS_FRAMES) as f64 / IRIS_FRAMES as f64),
            Self::HidePlayer { sprite, half_x, y } => scene.erase(sprite.id(), half_x, y),
        }
    }
}

// One-time reference measurement supplies score and note onsets in microseconds.
// Host playback uses these fixed times without modeling transition drawing work.
pub const BONUS_SCORE_US: [u64; 15] = [
    32, 59466, 118900, 178375, 237850, 297367, 356883, 416440, 476038, 535718, 595481, 655326,
    715334, 775630, 836460,
];
pub const BONUS_NOTE_US: [u64; 45] = [
    21256, 34011, 46765, 80690, 93445, 106199, 140124, 152892, 165661, 199599, 212368, 225136,
    259074, 271856, 284638, 318590, 331372, 344154, 378107, 390902, 403698, 437664, 450473, 463283,
    497262, 510099, 522935, 556942, 569806, 582671, 616705, 629596, 642488, 676549, 689496, 702442,
    736558, 749600, 762642, 796854, 810074, 823294, 857684, 871452, 885219,
];
pub const BONUS_DURATION_US: u64 = 898922;

#[derive(Clone, Copy, Debug)]
pub struct DeathNote {
    pub onset_us: u64,
    pub duration: u8,
    pub pitch: u8,
}

// Captured original countdown parameters retain the descending paired notes.
pub const DEATH_NOTES: [DeathNote; 24] = [
    DeathNote {
        onset_us: 10457,
        duration: 13,
        pitch: 240,
    },
    DeathNote {
        onset_us: 41048,
        duration: 13,
        pitch: 240,
    },
    DeathNote {
        onset_us: 71630,
        duration: 12,
        pitch: 224,
    },
    DeathNote {
        onset_us: 99693,
        duration: 12,
        pitch: 224,
    },
    DeathNote {
        onset_us: 127747,
        duration: 11,
        pitch: 208,
    },
    DeathNote {
        onset_us: 153283,
        duration: 11,
        pitch: 208,
    },
    DeathNote {
        onset_us: 178808,
        duration: 10,
        pitch: 192,
    },
    DeathNote {
        onset_us: 201816,
        duration: 10,
        pitch: 192,
    },
    DeathNote {
        onset_us: 224814,
        duration: 9,
        pitch: 176,
    },
    DeathNote {
        onset_us: 245281,
        duration: 9,
        pitch: 176,
    },
    DeathNote {
        onset_us: 265737,
        duration: 8,
        pitch: 160,
    },
    DeathNote {
        onset_us: 283675,
        duration: 8,
        pitch: 160,
    },
    DeathNote {
        onset_us: 301604,
        duration: 7,
        pitch: 144,
    },
    DeathNote {
        onset_us: 317001,
        duration: 7,
        pitch: 144,
    },
    DeathNote {
        onset_us: 332388,
        duration: 6,
        pitch: 128,
    },
    DeathNote {
        onset_us: 345255,
        duration: 6,
        pitch: 128,
    },
    DeathNote {
        onset_us: 358112,
        duration: 5,
        pitch: 112,
    },
    DeathNote {
        onset_us: 369337,
        duration: 5,
        pitch: 112,
    },
    DeathNote {
        onset_us: 380552,
        duration: 4,
        pitch: 96,
    },
    DeathNote {
        onset_us: 390121,
        duration: 4,
        pitch: 96,
    },
    DeathNote {
        onset_us: 399680,
        duration: 3,
        pitch: 80,
    },
    DeathNote {
        onset_us: 407580,
        duration: 3,
        pitch: 80,
    },
    DeathNote {
        onset_us: 415469,
        duration: 2,
        pitch: 64,
    },
    DeathNote {
        onset_us: 421685,
        duration: 2,
        pitch: 64,
    },
];
pub const DEATH_DURATION_US: u64 = 440313;
