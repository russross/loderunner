use crate::devices::speaker::Speaker;
use crate::event_timing::{Draw, Events, Model, TimingInterval};
use std::time::{Duration, Instant};

pub const NTSC_CPU_HZ: u64 = 1_023_000;

pub struct Clock {
    pub cycles: u64,
    pub events: Events,
    presentation_cycles: u64,
    pending_draws: Vec<Draw>,
    epoch: Instant,
    paced: bool,
}

impl Clock {
    pub fn new(paced: bool) -> Self {
        Self {
            cycles: 0,
            events: Events::default(),
            presentation_cycles: 0,
            pending_draws: Vec::new(),
            epoch: Instant::now(),
            paced,
        }
    }

    // Draws join ordered sound work at sound-section and interval boundaries.
    // Landing notes therefore retain their placement before pending graphics.
    pub fn record_draw(&mut self, draw: Draw) {
        self.pending_draws.push(draw);
    }

    pub fn flush_draws(&mut self) {
        for draw in self.pending_draws.drain(..) {
            self.events.draw(draw);
        }
    }

    pub fn begin_interval(&mut self) {
        self.events = Events::default();
        self.pending_draws.clear();
    }

    pub fn finish_events(&mut self) -> Events {
        self.flush_draws();
        std::mem::take(&mut self.events)
    }

    // The model commits a complete interval before audio submission and pacing.
    // Gameplay operations accumulate events without changing the shared clock.
    pub fn commit(&mut self, model: &Model, interval: &TimingInterval, speaker: &mut Speaker) {
        let cycles = model.render(interval, self.output_cycles(), speaker);
        self.cycles += cycles;
    }

    // Audio and host deadlines include application animation time. The gameplay
    // clock remains stationary while presentation advances between boards.
    pub fn output_cycles(&self) -> u64 {
        self.cycles + self.presentation_cycles
    }

    pub fn advance_presentation(&mut self, cycles: u64) {
        self.presentation_cycles += cycles;
    }

    // Absolute deadlines prevent host sleep overshoot from accumulating drift.
    // Headless execution commits the same intervals without sleeping.
    pub fn start_pacing(&mut self) {
        self.epoch = Instant::now() - self.output_duration();
    }

    fn output_duration(&self) -> Duration {
        Duration::from_secs_f64(self.output_cycles() as f64 / NTSC_CPU_HZ as f64)
    }

    pub fn remaining(&self) -> Option<Duration> {
        self.paced.then(|| {
            let deadline = self.epoch + self.output_duration();
            deadline.saturating_duration_since(Instant::now())
        })
    }

    pub fn checkpoint(&self) {
        if let Some(remaining) = self.remaining() {
            std::thread::sleep(remaining);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Clock, NTSC_CPU_HZ};
    use std::time::Duration;

    #[test]
    fn starting_pacing_excludes_host_initialization_and_retains_virtual_time() {
        let mut clock = Clock::new(true);
        clock.epoch -= Duration::from_secs(10);
        clock.cycles = NTSC_CPU_HZ * 3;
        clock.start_pacing();
        clock.cycles += NTSC_CPU_HZ;
        let remaining = clock.remaining().unwrap();
        assert!(remaining > Duration::from_millis(900));
        assert!(remaining <= Duration::from_secs(1));
        assert_eq!(clock.cycles, NTSC_CPU_HZ * 4);
    }

    #[test]
    fn unpaced_execution_never_requests_a_host_wait() {
        let mut clock = Clock::new(false);
        clock.start_pacing();
        clock.cycles = NTSC_CPU_HZ * 10;
        assert_eq!(clock.remaining(), None);
    }
}
