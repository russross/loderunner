//! Event counts describe work executed during an interval independently of its clock.
use crate::devices::speaker::Speaker;
use crate::routing::SearchWork;
use crate::sound::{NoteWork, render_note};
use lode_trace::Phase;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimingEvent {
    // Only features used by the calibrated model are retained. Search features
    // describe extents, never branch outcomes or original instruction paths.
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
    PlayerAttempt,
    Dig,
    Score,
    // Guard scheduling selects updates independently of route complexity. Search
    // counts preserve the number of cells and candidate rows actually examined.
    GuardUpdate,
    SameRowScan,
    LimitScan,
    RouteCandidate,
    AboveScan,
    BelowScan,
    // Hole counts describe timer work; input counts describe observed keys.
    HoleScan,
    ActiveHole,
    Input,
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
    pub fn search(&mut self, work: SearchWork) {
        for (event, count) in [
            (TimingEvent::SameRowScan, work.same_row_cells),
            (TimingEvent::LimitScan, work.horizontal_span),
            (TimingEvent::RouteCandidate, work.candidate_columns),
            (TimingEvent::AboveScan, work.upward_rows),
            (TimingEvent::BelowScan, work.downward_rows),
        ] {
            self.add(event, count);
        }
    }

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
        self.work.push(Work::Note(note));
    }

    pub fn begin_sound(&mut self, section: SoundSection) {
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

struct Budget {
    total: u64,
    graphics: u64,
    sound: u64,
    costs: Vec<u64>,
}

impl Model {
    pub fn calibrated() -> Self {
        serde_json::from_str(include_str!("model.json")).expect("embedded timing model")
    }

    // Fixed graphics and sound terms combine with the learned event corrections.
    // Polling is an explicit event; every phase uses this one estimation path.
    pub fn estimate(&self, interval: &TimingInterval) -> u64 {
        self.budget(interval).total
    }

    fn budget(&self, interval: &TimingInterval) -> Budget {
        let mut features = interval.events.counts.clone();
        let mut graphics = 0;
        let mut sound = 0;
        let mut costs = Vec::with_capacity(interval.events.work.len());
        for &work in &interval.events.work {
            let cost = self.work_cost(work);
            costs.push(cost);
            let feature = if matches!(work, Work::Draw(_)) {
                graphics += cost;
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
        Budget {
            total: (total.round().max(0.0) as u64).max(sound),
            graphics,
            sound,
            costs,
        }
    }

    pub fn render(&self, interval: &TimingInterval, start: u64, speaker: &mut Speaker) -> u64 {
        let budget = self.budget(interval);
        let non_sound = budget.total - budget.sound;
        let mut graphics_done = 0;
        let mut sound_done = 0;

        // Corrections adjust gaps between notes, while note countdowns retain
        // their original pitch, duration, muting and incoming-counter behavior.
        for (&work, cost) in interval.events.work.iter().zip(budget.costs) {
            let gap = if budget.graphics == 0 {
                non_sound
            } else {
                (u128::from(non_sound) * u128::from(graphics_done) / u128::from(budget.graphics))
                    as u64
            };
            match work {
                Work::Draw(_) => graphics_done += cost,
                Work::Note(note) => {
                    sound_done += render_note(note, |offset| {
                        speaker.access(start + gap + sound_done + offset)
                    });
                }
                _ => sound_done += cost,
            }
        }
        budget.total
    }

    fn work_cost(&self, work: Work) -> u64 {
        match work {
            Work::Draw(draw) => self.draw_cost(draw),
            Work::Note(note) => render_note(note, |_| {}),
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
        let wide = (u16::from(draw.half_x) * 2 % 7) >= 5;
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
}
