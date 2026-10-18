// Scores wrap at eight decimal digits; the status bar displays the lower seven.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Score(u32);

impl Score {
    pub fn new(value: u32) -> Self {
        assert!(value < 100_000_000);
        Self(value)
    }
    pub fn add(&mut self, points: u32) {
        self.0 = (self.0 + points % 100_000_000) % 100_000_000;
    }
    pub fn decimal(self) -> u32 {
        self.0
    }
    pub fn visible_digits(self) -> [u8; 7] {
        let mut digits = [0; 7];
        let mut value = self.0;
        for digit in digits.iter_mut().rev() {
            *digit = (value % 10) as u8;
            value /= 10;
        }
        digits
    }
}
