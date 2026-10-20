//! Round observations independent of graphics, storage, and game implementation.

pub mod apple2;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};

pub const FORMAT_VERSION: u32 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Cell {
    pub col: u8,
    pub row: u8,
}

impl Cell {
    pub fn new(col: u8, row: u8) -> Result<Self, TraceError> {
        if col >= 28 || row >= 16 {
            return Err(TraceError("cell outside the 28 by 16 playfield".into()));
        }
        Ok(Self { col, row })
    }
}

// Actor identities survive movement, death, and resurrection within an attempt.
// A new level attempt replaces the entire observation state at an explicit boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ActorId {
    Player,
    Guard(u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Facing {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Motion {
    Standing,
    Left,
    Right,
    Up,
    Down,
    Falling,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Action {
    None,
    DigLeft,
    DigRight,
    Trapped,
    ClimbingOut,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Life {
    Alive,
    Dead,
    Resurrecting,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Actor {
    pub id: ActorId,
    pub cell: Cell,
    pub x_adjust: u8,
    pub y_adjust: u8,
    pub facing: Facing,
    pub motion: Motion,
    pub action: Action,
    pub life: Life,
    pub carrying_gold: bool,
}

// Logical tiles and display sprites have different domains. This tile enum contains
// only the ten values admitted by the original level decoder.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[repr(u8)]
pub enum Tile {
    Empty,
    Brick,
    Stone,
    Ladder,
    Rope,
    Trap,
    HiddenLadder,
    Gold,
    Guard,
    Player,
}

impl Tile {
    pub fn decode(value: u8) -> Self {
        match value {
            1 => Self::Brick,
            2 => Self::Stone,
            3 => Self::Ladder,
            4 => Self::Rope,
            5 => Self::Trap,
            6 => Self::HiddenLadder,
            7 => Self::Gold,
            8 => Self::Guard,
            9 => Self::Player,
            _ => Self::Empty,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum HolePhase {
    Open,
    ClosingFirst,
    ClosingSecond,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hole {
    pub cell: Cell,
    pub generation: u32,
    pub phase: HolePhase,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Phase {
    Waiting,
    Paused,
    Playing,
    Died,
    LevelCleared,
    GameOver,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub level: u8,
    pub attempt: u32,
    pub phase: Phase,
    pub score: u32,
    pub lives_used: u32,
    pub exit_available: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub session: Session,
    pub actors: Vec<Actor>,
    pub active: Vec<Tile>,
    pub background: Vec<Tile>,
    pub hidden_ladders: Vec<Cell>,
    pub holes: Vec<Hole>,
}

impl Snapshot {
    pub fn canonicalize(&mut self) -> Result<(), TraceError> {
        self.actors.sort_by_key(|actor| actor.id);
        self.hidden_ladders.sort();
        self.holes.sort_by_key(|hole| hole.cell);
        self.validate()
    }

    pub fn validate(&self) -> Result<(), TraceError> {
        if self.active.len() != 448 || self.background.len() != 448 {
            return Err(TraceError(
                "snapshot maps must each contain 448 tiles".into(),
            ));
        }
        if !strictly_sorted(self.actors.iter().map(|actor| actor.id))
            || !strictly_sorted(self.hidden_ladders.iter().copied())
            || !strictly_sorted(self.holes.iter().map(|hole| hole.cell))
        {
            return Err(TraceError(
                "snapshot identities must be unique and sorted".into(),
            ));
        }
        if !self.actors.iter().any(|actor| actor.id == ActorId::Player) {
            return Err(TraceError("snapshot has no player".into()));
        }
        for actor in &self.actors {
            Cell::new(actor.cell.col, actor.cell.row)?;
            if actor.x_adjust > 4
                || actor.y_adjust > 4
                || matches!(actor.id, ActorId::Guard(slot) if !(1..=5).contains(&slot))
            {
                return Err(TraceError(
                    "invalid actor adjustment or guard identity".into(),
                ));
            }
        }
        for cell in self
            .hidden_ladders
            .iter()
            .chain(self.holes.iter().map(|hole| &hole.cell))
        {
            Cell::new(cell.col, cell.row)?;
        }
        Ok(())
    }
}

fn strictly_sorted<T: Ord>(values: impl Iterator<Item = T>) -> bool {
    let mut previous = None;
    for value in values {
        if previous.as_ref().is_some_and(|old| old >= &value) {
            return false;
        }
        previous = Some(value);
    }
    true
}

// Inputs are delivery occurrences rather than changes in held-key state. Ordinals
// address logical observations and let repeated identical codes remain distinct.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Input {
    pub observation: u32,
    pub key: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Subject {
    Actor(ActorId),
    Cell(Cell),
    Session,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Occurrence {
    Contact { other: Subject },
    GoldCollected,
    GoldDropped { cell: Cell },
    Landed,
    Trapped,
}

fn canonical_events(events: Vec<Occurrence>) -> Vec<Occurrence> {
    let mut canonical = Vec::new();
    let mut contacts = Vec::new();
    let flush = |contacts: &mut Vec<Subject>, output: &mut Vec<Occurrence>| {
        contacts.sort();
        contacts.dedup();
        output.extend(
            contacts
                .drain(..)
                .map(|other| Occurrence::Contact { other }),
        );
    };
    for event in events {
        match event {
            Occurrence::Contact { other } => contacts.push(other),
            event => {
                flush(&mut contacts, &mut canonical);
                canonical.push(event);
            }
        }
    }
    flush(&mut contacts, &mut canonical);
    canonical
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Episode {
    pub subject: Subject,
    pub events: Vec<Occurrence>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Change {
    Session {
        before: Session,
        after: Session,
    },
    Actor {
        id: ActorId,
        before: Option<Actor>,
        after: Option<Actor>,
    },
    Tile {
        cell: Cell,
        active: bool,
        before: Tile,
        after: Tile,
    },
    HiddenLadders {
        before: Vec<Cell>,
        after: Vec<Cell>,
    },
    Holes {
        before: Vec<Hole>,
        after: Vec<Hole>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Round {
    pub number: u64,
    pub start_delta: u64,
    pub inputs: Vec<Input>,
    pub changes: Vec<Change>,
    pub episodes: Vec<Episode>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Header {
    pub version: u32,
    pub source: String,
    pub configuration: String,
    pub initial: Snapshot,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trace {
    pub header: Header,
    pub rounds: Vec<Round>,
    pub final_delta: u64,
    pub final_state: Snapshot,
}

// Changes are generated in one stable order. Their preconditions are checked during
// trace application, and the same generator checks that no redundant edits survive.
fn differences(before: &Snapshot, after: &Snapshot) -> Vec<Change> {
    let mut changes = Vec::new();
    if before.session != after.session {
        changes.push(Change::Session {
            before: before.session.clone(),
            after: after.session.clone(),
        });
    }
    let old: BTreeMap<_, _> = before.actors.iter().map(|a| (a.id, a)).collect();
    let new: BTreeMap<_, _> = after.actors.iter().map(|a| (a.id, a)).collect();
    let mut ids: Vec<_> = old.keys().chain(new.keys()).copied().collect();
    ids.sort();
    ids.dedup();
    for id in ids {
        if old.get(&id) != new.get(&id) {
            changes.push(Change::Actor {
                id,
                before: old.get(&id).map(|a| (*a).clone()),
                after: new.get(&id).map(|a| (*a).clone()),
            });
        }
    }
    for index in 0..448 {
        for active in [true, false] {
            let old = if active {
                before.active[index]
            } else {
                before.background[index]
            };
            let new = if active {
                after.active[index]
            } else {
                after.background[index]
            };
            if old != new {
                changes.push(Change::Tile {
                    cell: Cell {
                        col: (index % 28) as u8,
                        row: (index / 28) as u8,
                    },
                    active,
                    before: old,
                    after: new,
                });
            }
        }
    }
    if before.hidden_ladders != after.hidden_ladders {
        changes.push(Change::HiddenLadders {
            before: before.hidden_ladders.clone(),
            after: after.hidden_ladders.clone(),
        });
    }
    if before.holes != after.holes {
        changes.push(Change::Holes {
            before: before.holes.clone(),
            after: after.holes.clone(),
        });
    }
    changes
}

pub struct Observer {
    state: Snapshot,
    previous_start: u64,
    number: u64,
}

impl Observer {
    pub fn new(mut initial: Snapshot) -> Result<Self, TraceError> {
        initial.canonicalize()?;
        Ok(Self {
            state: initial,
            previous_start: 0,
            number: 0,
        })
    }

    pub fn finish_round(
        &mut self,
        start: u64,
        mut after: Snapshot,
        inputs: Vec<Input>,
        occurrences: Vec<(Subject, Occurrence)>,
    ) -> Result<Round, TraceError> {
        after.canonicalize()?;
        if !strictly_sorted(inputs.iter().map(|input| input.observation)) {
            return Err(TraceError(
                "input observations must increase within a round".into(),
            ));
        }
        let start_delta = start
            .checked_sub(self.previous_start)
            .ok_or_else(|| TraceError("round clock moved backwards".into()))?;
        let mut groups: BTreeMap<Subject, Vec<Occurrence>> = BTreeMap::new();
        for (subject, event) in occurrences {
            groups.entry(subject).or_default().push(event);
        }
        let round = Round {
            number: self.number,
            start_delta,
            inputs,
            changes: differences(&self.state, &after),
            episodes: groups
                .into_iter()
                .map(|(subject, events)| Episode {
                    subject,
                    events: canonical_events(events),
                })
                .collect(),
        };
        self.state = after;
        self.previous_start = start;
        self.number += 1;
        Ok(round)
    }
}

impl Trace {
    pub fn to_json(&self) -> Result<String, TraceError> {
        self.validate()?;
        serde_json::to_string_pretty(self).map_err(|error| TraceError(error.to_string()))
    }

    pub fn from_json(text: &str) -> Result<Self, TraceError> {
        let trace: Self =
            serde_json::from_str(text).map_err(|error| TraceError(error.to_string()))?;
        trace.validate()?;
        Ok(trace)
    }

    pub fn validate(&self) -> Result<(), TraceError> {
        if self.header.version != FORMAT_VERSION {
            return Err(TraceError("unsupported trace version".into()));
        }
        self.header.initial.validate()?;
        let mut state = self.header.initial.clone();
        let mut clock = 0_u64;
        for (number, round) in self.rounds.iter().enumerate() {
            if round.number != number as u64
                || !strictly_sorted(round.inputs.iter().map(|input| input.observation))
                || !strictly_sorted(round.episodes.iter().map(|episode| episode.subject))
                || round
                    .episodes
                    .iter()
                    .any(|episode| episode.events.is_empty())
                || round
                    .episodes
                    .iter()
                    .any(|episode| canonical_events(episode.events.clone()) != episode.events)
            {
                return Err(TraceError(format!(
                    "round {number}: invalid ordering or identity"
                )));
            }
            clock = clock
                .checked_add(round.start_delta)
                .ok_or_else(|| TraceError("trace clock overflow".into()))?;
            let before = state.clone();
            for change in &round.changes {
                apply_change(&mut state, change)?;
            }
            state.canonicalize()?;
            if differences(&before, &state) != round.changes {
                return Err(TraceError(format!(
                    "round {number}: changes are not canonical"
                )));
            }
        }
        clock
            .checked_add(self.final_delta)
            .ok_or_else(|| TraceError("trace clock overflow".into()))?;
        self.final_state.validate()?;
        if state != self.final_state {
            return Err(TraceError(
                "final state differs from applied changes".into(),
            ));
        }
        Ok(())
    }
}

fn replace<T: Clone + PartialEq>(value: &mut T, before: &T, after: &T) -> Result<(), TraceError> {
    if value != before {
        return Err(TraceError("state change precondition failed".into()));
    }
    *value = after.clone();
    Ok(())
}

fn apply_change(state: &mut Snapshot, change: &Change) -> Result<(), TraceError> {
    match change {
        Change::Session { before, after } => replace(&mut state.session, before, after),
        Change::Actor { id, before, after } => {
            let found = state.actors.iter().position(|actor| actor.id == *id);
            if found.map(|i| &state.actors[i]) != before.as_ref()
                || after.as_ref().is_some_and(|actor| actor.id != *id)
            {
                return Err(TraceError("actor change precondition failed".into()));
            }
            if let Some(index) = found {
                state.actors.remove(index);
            }
            if let Some(actor) = after {
                state.actors.push(actor.clone());
            }
            Ok(())
        }
        Change::Tile {
            cell,
            active,
            before,
            after,
        } => {
            Cell::new(cell.col, cell.row)?;
            let map = if *active {
                &mut state.active
            } else {
                &mut state.background
            };
            replace(
                &mut map[usize::from(cell.row) * 28 + usize::from(cell.col)],
                before,
                after,
            )
        }
        Change::HiddenLadders { before, after } => {
            replace(&mut state.hidden_ladders, before, after)
        }
        Change::Holes { before, after } => replace(&mut state.holes, before, after),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceError(pub String);

impl Display for TraceError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for TraceError {}
