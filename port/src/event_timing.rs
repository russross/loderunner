//! Event counts describe work executed during an interval independently of its clock.
use crate::devices::speaker::Speaker;
use lode_trace::Phase;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimingEvent {
    // Interval markers separate fixed dispatch from terminal paths. Local helper
    // cycle totals retain variable alignment and sound-countdown work.
    Round,
    Poll,
    Pause,
    Death,
    Completion,
    GraphicsCycles,
    SoundCycles,
    TileDraw,
    ActorDraw,
    SpriteErase,
    SoundSection,
    Note,
    // Player attempts, rendering and collection checks count executed work even
    // when movement is blocked or a centered cell contains no gold.
    PlayerFall,
    PlayerAttempt,
    PlayerAnimation,
    PlayerDraw,
    GoldCheck,
    Dig,
    Score,
    // Guard scheduling selects updates independently of route complexity. Search
    // counts preserve the number of cells and candidate rows actually examined.
    GuardUpdate,
    GuardFall,
    GuardAttempt,
    GuardRoute,
    SameRowScan,
    LimitScan,
    RouteCandidate,
    AboveScan,
    BelowScan,
    // Timer scans and active entries distinguish empty boards from pending holes
    // and resurrection work; input counts reflect device observations.
    HoleScan,
    ActiveHole,
    ResurrectionScan,
    ActiveResurrection,
    Exit,
    Input,
    // Vertical searches use different terrain and neighbor paths. These probes
    // retain early returns and the original left/right pointer asymmetry.
    BelowNonEmpty,
    BelowLeftProbe,
    BelowRightProbe,
    AboveLeftProbe,
    AboveRightProbe,
    RouteDown,
    RouteUp,
    HorizontalSearch,
    SameRowSupport,
    // Candidate distances and improvements expose branch work that cell counts
    // alone cannot distinguish when guards search relative to the player.
    DistanceSameRow,
    DistanceAbove,
    DistanceBelow,
    BetterRoute,
    BelowPlayerProbe,
    AbovePlayerProbe,
}

// Draw records retain only inputs that affect the model's fixed graphics terms.
// Gameplay uses the same parameters for pixel work without evaluating durations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DrawKind {
    Tile,
    Actor,
    Erase,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Draw {
    pub kind: DrawKind,
    pub sprite: u8,
    pub half_x: u8,
    pub y: u8,
    pub shift: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteWork {
    pub enabled: bool,
    pub pitch: u8,
    pub duration: u8,
    pub low_counter: u8,
}

// Section boundaries retain buffered-note padding and falling-note setup. Notes
// remain separate records so landing and transition tunes share the same path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SoundSection {
    Buffered { duration: u8, period: u8 },
    Falling,
    Idle { period: u8 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Work {
    Draw(Draw),
    Note(NoteWork),
    BeginSound(SoundSection),
    EndSound(SoundSection),
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Events {
    counts: BTreeMap<TimingEvent, u64>,
    pub work: Vec<Work>,
}

impl Events {
    pub fn add(&mut self, event: TimingEvent, count: u64) {
        *self.counts.entry(event).or_default() += count;
    }

    pub fn count(&self, event: TimingEvent) -> u64 {
        self.counts.get(&event).copied().unwrap_or(0)
    }

    // Ordered work records let the model place audio within the committed turn.
    // Their counts also supply the empirically calibrated correction factors.
    pub fn draw(&mut self, draw: Draw) {
        self.add(
            match draw.kind {
                DrawKind::Tile => TimingEvent::TileDraw,
                DrawKind::Actor => TimingEvent::ActorDraw,
                DrawKind::Erase => TimingEvent::SpriteErase,
            },
            1,
        );
        self.work.push(Work::Draw(draw));
    }

    pub fn note(&mut self, note: NoteWork) {
        self.add(TimingEvent::Note, 1);
        self.work.push(Work::Note(note));
    }

    pub fn begin_sound(&mut self, section: SoundSection) {
        self.add(TimingEvent::SoundSection, 1);
        self.work.push(Work::BeginSound(section));
    }

    pub fn end_sound(&mut self, section: SoundSection) {
        self.work.push(Work::EndSound(section));
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TimingInterval {
    pub phase: Phase,
    pub end_phase: Phase,
    pub events: Events,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Model {
    pub coefficients: BTreeMap<TimingEvent, f64>,
}

impl Model {
    pub fn calibrated() -> Self {
        serde_json::from_str(include_str!("model.json")).expect("embedded timing model")
    }

    // Fixed graphics and sound terms combine with the learned event corrections.
    // Polling is an explicit event; every phase uses this one estimation path.
    pub fn estimate(&self, interval: &TimingInterval) -> u64 {
        let mut features = interval.events.counts.clone();
        let mut sound = 0;
        for &work in &interval.events.work {
            let cost = self.work_cost(work);
            let feature = if matches!(work, Work::Draw(_)) {
                TimingEvent::GraphicsCycles
            } else {
                sound += cost;
                TimingEvent::SoundCycles
            };
            *features.entry(feature).or_default() += cost;
        }
        let total = features
            .iter()
            .map(|(event, count)| {
                self.coefficients.get(event).copied().unwrap_or(0.0) * *count as f64
            })
            .sum::<f64>();
        // Audio countdowns must fit without changing pitch. Calibrated gameplay
        // has spare non-audio time; this bound also protects sparse custom turns.
        (total.round().max(0.0) as u64).max(sound)
    }

    pub fn render(&self, interval: &TimingInterval, start: u64, speaker: &mut Speaker) -> u64 {
        let total = self.estimate(interval);
        let graphics = interval
            .events
            .work
            .iter()
            .filter(|work| matches!(work, Work::Draw(_)))
            .map(|work| self.work_cost(*work))
            .sum::<u64>();
        let sound = interval
            .events
            .work
            .iter()
            .filter(|work| !matches!(work, Work::Draw(_)))
            .map(|work| self.work_cost(*work))
            .sum::<u64>();
        let non_sound = total - sound;
        let mut graphics_done = 0;
        let mut sound_done = 0;

        // Corrections adjust gaps between notes, while note countdowns retain
        // their original pitch, duration, muting and incoming-counter behavior.
        for &work in &interval.events.work {
            let gap = if graphics == 0 {
                non_sound
            } else {
                (u128::from(non_sound) * u128::from(graphics_done) / u128::from(graphics)) as u64
            };
            match work {
                Work::Draw(_) => graphics_done += self.work_cost(work),
                Work::Note(note) => {
                    sound_done += self.note_cost(note, |offset| {
                        speaker.access(start + gap + sound_done + offset)
                    });
                }
                _ => sound_done += self.work_cost(work),
            }
        }
        total
    }

    fn work_cost(&self, work: Work) -> u64 {
        match work {
            Work::Draw(draw) => self.draw_cost(draw),
            Work::Note(note) => self.note_cost(note, |_| {}),
            Work::BeginSound(SoundSection::Buffered { .. }) => 19,
            Work::BeginSound(SoundSection::Falling) => 34,
            Work::BeginSound(SoundSection::Idle { period }) => 40 + 1812 * (u64::from(period) + 1),
            Work::EndSound(SoundSection::Buffered { duration, period }) => {
                match period.cmp(&duration) {
                    std::cmp::Ordering::Equal => 28,
                    std::cmp::Ordering::Less => 30,
                    std::cmp::Ordering::Greater => 42 + 906 * u64::from(period - duration),
                }
            }
            Work::EndSound(SoundSection::Falling) => 8,
            Work::EndSound(SoundSection::Idle { .. }) => 0,
        }
    }

    fn draw_cost(&self, draw: Draw) -> u64 {
        let shifted = 1824
            + (0..22)
                .filter(|plane| (plane * 104) % 256 + u64::from(draw.sprite) >= 256)
                .count() as u64;
        let row_crossings = (draw.y..draw.y + 11)
            .map(|row| u64::from(row >= 123) + u64::from(row >= 187))
            .sum::<u64>();
        let wide = draw.shift >= 5;
        match draw.kind {
            DrawKind::Tile => 125 + shifted + 253 + row_crossings + 11 * if wide { 81 } else { 69 },
            DrawKind::Actor => {
                69 + u64::from(draw.half_x >= 102)
                    + shifted
                    + 2 * row_crossings
                    + 11 * if wide { 170 } else { 133 }
            }
            DrawKind::Erase => {
                66 + u64::from(draw.half_x >= 102)
                    + shifted
                    + 2 * row_crossings
                    + 11 * if wide { 131 } else { 107 }
            }
        }
    }

    // Note work is evaluated without a second clock. Runs end only at speaker
    // reloads or low-counter wraps, including the original zero-byte wrap case.
    fn note_cost(&self, note: NoteWork, mut edge: impl FnMut(u64)) -> u64 {
        let count = |byte: u8| if byte == 0 { 256_u64 } else { u64::from(byte) };
        let mut remaining = (count(note.duration) - 1) * 256 + count(note.low_counter);
        let mut until_wrap = count(note.low_counter);
        let mut pitch_counter = count(note.duration);
        let mut cost = 6;
        while remaining > 0 {
            cost += if note.enabled { 9 } else { 6 };
            if note.enabled {
                edge(cost);
            }
            loop {
                let steps = pitch_counter.min(until_wrap).min(remaining);
                remaining -= steps;
                until_wrap -= steps;
                pitch_counter -= steps;
                if remaining == 0 {
                    return cost + (steps - 1) * 10 + 18;
                }
                cost += steps * 10;
                if until_wrap == 0 {
                    cost += 6;
                    until_wrap = 256;
                }
                if pitch_counter == 0 {
                    cost += 5;
                    pitch_counter = count(note.pitch);
                    break;
                }
            }
        }
        cost
    }
}
