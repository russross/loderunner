pub const ESCAPE: u8 = 0x9b;
pub const RETURN: u8 = 0x8d;
pub const LEFT_ARROW: u8 = 0x88;
pub const RIGHT_ARROW: u8 = 0x95;

// Application navigation is intercepted before gameplay observes the latch.
// Its codes retain instantaneous board changes in recordings and trace metadata.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum LevelNavigation {
    Previous = 0x82,
    Next = 0x83,
}

impl LevelNavigation {
    pub fn from_key(code: u8) -> Option<Self> {
        match code {
            0x82 => Some(Self::Previous),
            0x83 => Some(Self::Next),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Keyboard {
    code: u8,
    available: bool,
}

impl Keyboard {
    pub fn deliver(&mut self, code: u8) {
        self.code = code & 0x7f;
        self.available = true;
    }

    pub fn read(&self) -> u8 {
        self.code | if self.available { 0x80 } else { 0 }
    }

    // Acknowledgement retains the last code. Physical key release does not
    // provide an observation and therefore cannot cancel persistent movement.
    pub fn acknowledge(&mut self) {
        self.available = false;
    }

    pub fn observe(&mut self) -> Option<u8> {
        let code = self.available.then(|| self.read());
        self.acknowledge();
        code
    }
}

// Codes remain the external trace/input format; gameplay dispatch uses named
// controls. A control leaves the player's previous movement command intact.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Control {
    Sacrifice = 0x81,
    Slower = LEFT_ARROW,
    Keyboard = 0x8b,
    EndGame = 0x92,
    ToggleSound = 0x93,
    Faster = RIGHT_ARROW,
    Pause = ESCAPE,
    SkipLevel = 0x9e,
    Joystick = 0x8a,
}

impl Control {
    pub fn from_key(code: u8) -> Option<Self> {
        Some(match code {
            0x81 => Self::Sacrifice,
            0x88 => Self::Slower,
            0x8b => Self::Keyboard,
            0x92 => Self::EndGame,
            0x93 => Self::ToggleSound,
            0x95 => Self::Faster,
            0x9b => Self::Pause,
            0x9e => Self::SkipLevel,
            0x8a | 0x98 | 0x99 => Self::Joystick,
            _ => return None,
        })
    }
}
