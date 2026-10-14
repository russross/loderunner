use crate::devices::speaker::Speaker;
use crate::event_timing::{Events, Model, TimingInterval};
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

    // The model commits a complete interval before audio submission and pacing.
    // Gameplay operations accumulate events without changing the shared clock.
    pub fn commit(&mut self, model: &Model, interval: &TimingInterval, speaker: &mut Speaker) {
        let cycles = model.render(interval, self.cycles, speaker);
        self.cycles += cycles;
    }

    // Absolute deadlines prevent host sleep overshoot from accumulating drift.
    // Headless execution commits the same intervals without sleeping.
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
