#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Score(pub [u8; 4]);

impl Score {
    // Each packed pair is added with decimal carry, least significant pair first.
    // Overflow of the fourth pair wraps just as the original four-byte score does.
    pub fn add(&mut self, low: u8, high: u8) {
        let mut carry = 0;
        for (index, pair) in self.0.iter_mut().enumerate() {
            let addend = match index {
                0 => low,
                1 => high,
                _ => 0,
            };
            let unpack = |value: u8| u16::from(value >> 4) * 10 + u16::from(value & 15);
            let sum = unpack(*pair) + unpack(addend) + carry;
            carry = u16::from(sum >= 100);
            let value = (sum % 100) as u8;
            *pair = ((value / 10) << 4) | (value % 10);
        }
    }

    pub fn decimal(&self) -> u32 {
        self.0.iter().rev().fold(0, |total, pair| {
            total * 100 + u32::from(pair >> 4) * 10 + u32::from(pair & 15)
        })
    }
}
