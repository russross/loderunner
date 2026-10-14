//! Event counts describe work executed during an interval independently of its clock.
use lode_trace::Phase;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimingEvent {
    // Interval markers separate fixed dispatch from terminal paths. Local helper
    // cycle totals retain variable alignment and sound-countdown work.
    Round,
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

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Events(pub BTreeMap<TimingEvent, u64>);

impl Events {
    pub fn add(&mut self, event: TimingEvent, count: u64) {
        *self.0.entry(event).or_default() += count;
    }

    pub fn merge(&mut self, other: Self) {
        for (event, count) in other.0 {
            self.add(event, count);
        }
    }

    pub fn count(&self, event: TimingEvent) -> u64 {
        self.0.get(&event).copied().unwrap_or(0)
    }
}

// One row closes a complete interval. Non-gameplay intervals retain the existing
// budgets because disk loading and presentation do not have native equivalents.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TimingInterval {
    pub phase: Phase,
    pub end_phase: Phase,
    pub legacy: u64,
    pub events: Events,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Model {
    pub coefficients: BTreeMap<TimingEvent, f64>,
}

impl Model {
    pub fn calibrated() -> Self {
        serde_json::from_str(include_str!("../../sources/analysis/event-model.json"))
            .expect("embedded timing model")
    }

    pub fn estimate(&self, interval: &TimingInterval) -> u64 {
        if interval.phase != Phase::Playing
            || !matches!(
                interval.end_phase,
                Phase::Playing | Phase::Died | Phase::LevelCleared
            )
        {
            return interval.legacy;
        }
        interval
            .events
            .0
            .iter()
            .map(|(event, count)| {
                self.coefficients.get(event).copied().unwrap_or(0.0) * *count as f64
            })
            .sum::<f64>()
            .round()
            .max(0.0) as u64
    }
}
