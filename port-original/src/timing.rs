use crate::event_timing::Events;
use std::time::{Duration, Instant};

pub const NTSC_CPU_HZ: u64 = 1_023_000;

pub struct Clock {
    pub cycles: u64,
    pub events: Events,
    epoch: Instant,
    paced: bool,
}

impl Clock {
    pub fn new(paced: bool) -> Self {
        Self {
            cycles: 0,
            events: Events::default(),
            epoch: Instant::now(),
            paced,
        }
    }

    pub fn advance(&mut self, cycles: u64) {
        self.cycles += cycles;
    }

    // Pacing uses an absolute deadline, so sleep overshoot does not become extra
    // simulated time. Headless replay advances the same clock without sleeping.
    pub fn checkpoint(&self) {
        if self.paced {
            let deadline =
                self.epoch + Duration::from_secs_f64(self.cycles as f64 / NTSC_CPU_HZ as f64);
            if let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
                std::thread::sleep(remaining);
            }
        }
    }
}

// These costs include the callee's RTS but exclude the caller's JSR. A zero byte
// counter decrements through 256 iterations in the original NMOS 6502 loops.
pub fn sound_delay1(counter: u8) -> u64 {
    let count = if counter == 0 {
        256
    } else {
        u64::from(counter)
    };
    906 * count + 5
}

pub fn sound_delay(setting: u8) -> u64 {
    assert!(setting <= 15);
    906 * (2 * (u64::from(setting) + 1)) + 11
}

pub fn row_to_addr(row: u8) -> u64 {
    23 + u64::from(row >= 123) + u64::from(row >= 187)
}
