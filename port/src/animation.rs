//! Player and guard frames keep separate sequences and original sprite identities.
use crate::actors::Direction;
use crate::graphics::Sprite;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum PlayerFrame {
    WalkLeft1 = 11,
    WalkLeft2 = 12,
    WalkLeft3 = 13,
    RopeLeft1 = 24,
    RopeLeft2 = 25,
    RopeLeft3 = 26,
    DigLeft = 15,
    FallLeft = 19,
    WalkRight1 = 9,
    WalkRight2 = 16,
    WalkRight3 = 17,
    RopeRight1 = 21,
    RopeRight2 = 22,
    RopeRight3 = 23,
    DigRight = 37,
    FallRight = 20,
    Climb1 = 14,
    Climb2 = 18,
}

impl PlayerFrame {
    pub fn sprite(self) -> Sprite {
        Sprite::reference(self as u8)
    }

    // Continuing a movement sequence advances its frame. Entering a different
    // sequence starts at its first frame, including after falling or digging.
    pub fn advance(&mut self, direction: Direction, rope: bool) {
        let sequence: &[Self] = match (direction, rope) {
            (Direction::Left, false) => &[Self::WalkLeft1, Self::WalkLeft2, Self::WalkLeft3],
            (Direction::Left, true) => &[Self::RopeLeft1, Self::RopeLeft2, Self::RopeLeft3],
            (Direction::Right, false) => &[Self::WalkRight1, Self::WalkRight2, Self::WalkRight3],
            (Direction::Right, true) => &[Self::RopeRight1, Self::RopeRight2, Self::RopeRight3],
            _ => &[Self::Climb1, Self::Climb2],
        };
        let next = sequence
            .iter()
            .position(|frame| frame == self)
            .map_or(0, |index| (index + 1) % sequence.len());
        *self = sequence[next];
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum GuardFrame {
    WalkLeft1 = 8,
    WalkLeft2 = 43,
    WalkLeft3 = 44,
    RopeLeft1 = 48,
    RopeLeft2 = 49,
    RopeLeft3 = 50,
    FallLeft = 54,
    WalkRight1 = 40,
    WalkRight2 = 41,
    WalkRight3 = 42,
    RopeRight1 = 45,
    RopeRight2 = 46,
    RopeRight3 = 47,
    FallRight = 53,
    Climb1 = 51,
    Climb2 = 52,
}

impl GuardFrame {
    pub fn sprite(self) -> Sprite {
        Sprite::reference(self as u8)
    }

    // Guard frames follow their own walking, rope and climbing sequences. The
    // selected update advances animation only after a permitted movement.
    pub fn advance(&mut self, direction: Direction, rope: bool) {
        let sequence: &[Self] = match (direction, rope) {
            (Direction::Left, false) => &[Self::WalkLeft1, Self::WalkLeft2, Self::WalkLeft3],
            (Direction::Left, true) => &[Self::RopeLeft1, Self::RopeLeft2, Self::RopeLeft3],
            (Direction::Right, false) => &[Self::WalkRight1, Self::WalkRight2, Self::WalkRight3],
            (Direction::Right, true) => &[Self::RopeRight1, Self::RopeRight2, Self::RopeRight3],
            _ => &[Self::Climb1, Self::Climb2],
        };
        let next = sequence
            .iter()
            .position(|frame| frame == self)
            .map_or(0, |index| (index + 1) % sequence.len());
        *self = sequence[next];
    }
}
