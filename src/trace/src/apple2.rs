//! Apple II observations decoded into the same object model as native traces.

use crate::{
    Action, Actor, ActorId, Cell, Facing, Hole, HolePhase, Input, Life, Motion, Occurrence, Phase,
    Session, Snapshot, Subject, Tile, TraceError,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Signal {
    Motion { actor: ActorId, motion: Motion },
    SelectGuard { slot: u8 },
    Collected { actor: ActorId },
    Dropped { slot: u8, cell: Cell },
    Contact { actor: ActorId, other: Subject },
    Landed,
    Trapped { slot: u8 },
    Exit,
    NewGame,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Sample {
    pub cycle: u64,
    pub phase: Phase,
    pub attempt: u32,
    pub memory: String,
    pub inputs: Vec<Input>,
    pub signals: Vec<Signal>,
}

pub struct Observation {
    pub snapshot: Snapshot,
    pub inputs: Vec<Input>,
    pub occurrences: Vec<(Subject, Occurrence)>,
}

// These are observation counters, not gameplay simulation. Motion follows the
// executed source path, and hole identities follow openings in sampled state.
pub struct Apple2Observer {
    motions: [Motion; 6],
    generations: [u32; 448],
    open: BTreeSet<Cell>,
    attempt: u32,
    exit: bool,
}

impl Default for Apple2Observer {
    fn default() -> Self {
        Self {
            motions: [Motion::Standing; 6],
            generations: [0; 448],
            open: BTreeSet::new(),
            attempt: 0,
            exit: false,
        }
    }
}

impl Apple2Observer {
    pub fn observe(&mut self, sample: &Sample) -> Result<Observation, TraceError> {
        let memory = decode_memory(&sample.memory)?;
        if sample.attempt != self.attempt {
            self.attempt = sample.attempt;
            self.motions.fill(Motion::Standing);
            self.generations.fill(0);
            self.open.clear();
            self.exit = false;
        }
        let mut occurrences = Vec::new();
        for signal in &sample.signals {
            match signal {
                Signal::Motion { actor, motion } => self.motions[actor_index(*actor)?] = *motion,
                Signal::SelectGuard { slot } => {
                    self.motions[actor_index(ActorId::Guard(*slot))?] = Motion::Standing
                }
                Signal::Collected { actor } => {
                    actor_index(*actor)?;
                    occurrences.push((Subject::Actor(*actor), Occurrence::GoldCollected));
                }
                Signal::Dropped { slot, cell } => {
                    actor_index(ActorId::Guard(*slot))?;
                    Cell::new(cell.col, cell.row)?;
                    occurrences.push((
                        Subject::Actor(ActorId::Guard(*slot)),
                        Occurrence::GoldDropped { cell: *cell },
                    ));
                }
                Signal::Contact { actor, other } => {
                    actor_index(*actor)?;
                    match other {
                        Subject::Actor(id) => {
                            actor_index(*id)?;
                        }
                        Subject::Cell(cell) => {
                            Cell::new(cell.col, cell.row)?;
                        }
                        Subject::Session => {}
                    }
                    occurrences.push((
                        Subject::Actor(*actor),
                        Occurrence::Contact { other: *other },
                    ));
                }
                Signal::Landed => {
                    occurrences.push((Subject::Actor(ActorId::Player), Occurrence::Landed))
                }
                Signal::Trapped { slot } => {
                    actor_index(ActorId::Guard(*slot))?;
                    occurrences.push((Subject::Actor(ActorId::Guard(*slot)), Occurrence::Trapped));
                }
                Signal::Exit => self.exit = true,
                Signal::NewGame => {
                    self.motions.fill(Motion::Standing);
                    self.generations.fill(0);
                    self.open.clear();
                    self.exit = false;
                }
            }
        }

        // Actor flags are read from persistent arrays. Working guard scratch
        // registers may belong to a different guard at a committed boundary.
        let mut actors = vec![Actor {
            id: ActorId::Player,
            cell: Cell::new(memory[0], memory[1])?,
            x_adjust: memory[2],
            y_adjust: memory[3],
            facing: facing(memory[5]),
            motion: self.motions[0],
            action: match memory[0x9c] {
                0xff => Action::DigLeft,
                1 => Action::DigRight,
                _ => Action::None,
            },
            life: if memory[0x9a] == 0 {
                Life::Dead
            } else {
                Life::Alive
            },
            carrying_gold: false,
        }];
        if memory[0x8d] > 5 {
            return Err(TraceError("machine has too many guards".into()));
        }
        for slot in 1..=memory[0x8d] {
            let i = usize::from(slot);
            let timer = memory[0xc70 + i];
            let resurrecting = memory[0xc98 + i] != 0;
            actors.push(Actor {
                id: ActorId::Guard(slot),
                cell: Cell::new(memory[0xc60 + i], memory[0xc68 + i])?,
                x_adjust: memory[0xc78 + i],
                y_adjust: memory[0xc80 + i],
                facing: facing(memory[0xc90 + i]),
                motion: self.motions[i],
                action: if resurrecting {
                    Action::None
                } else if (1..=6).contains(&timer) {
                    Action::ClimbingOut
                } else if (7..0x7f).contains(&timer) {
                    Action::Trapped
                } else {
                    Action::None
                },
                life: if resurrecting {
                    Life::Resurrecting
                } else {
                    Life::Alive
                },
                carrying_gold: timer & 0x80 != 0,
            });
        }

        let mut holes = Vec::new();
        let mut open = BTreeSet::new();
        for slot in 0..31 {
            let timer = memory[0xce0 + slot];
            if timer == 0 {
                continue;
            }
            let cell = Cell::new(memory[0xca0 + slot], memory[0xcc0 + slot])?;
            let index = usize::from(cell.row) * 28 + usize::from(cell.col);
            if !self.open.contains(&cell) {
                self.generations[index] = self.generations[index]
                    .checked_add(1)
                    .ok_or_else(|| TraceError("hole generation overflow".into()))?;
            }
            open.insert(cell);
            holes.push(Hole {
                cell,
                generation: self.generations[index],
                phase: if timer <= 10 {
                    HolePhase::ClosingSecond
                } else if timer <= 20 {
                    HolePhase::ClosingFirst
                } else {
                    HolePhase::Open
                },
            });
        }
        self.open = open;
        let mut hidden_ladders = Vec::new();
        if memory[0xa3] > 45 {
            return Err(TraceError("machine has too many hidden ladders".into()));
        }
        for slot in 1..=usize::from(memory[0xa3]) {
            if memory[0xc00 + slot] & 0x80 == 0 {
                hidden_ladders.push(Cell::new(memory[0xc00 + slot], memory[0xc30 + slot])?);
            }
        }

        let mut snapshot = Snapshot {
            session: Session {
                level: memory[0xa6],
                attempt: sample.attempt,
                phase: sample.phase,
                score: decimal(memory[0x8e..0x92].iter().rev().copied())?,
                lives: memory[0x98],
                exit_available: self.exit,
            },
            actors,
            active: map(&memory[0x800..0x9c0])?,
            background: map(&memory[0xa00..0xbc0])?,
            hidden_ladders,
            holes,
        };
        snapshot.canonicalize()?;
        Ok(Observation {
            snapshot,
            inputs: sample.inputs.clone(),
            occurrences,
        })
    }
}

fn actor_index(actor: ActorId) -> Result<usize, TraceError> {
    match actor {
        ActorId::Player => Ok(0),
        ActorId::Guard(slot @ 1..=5) => Ok(usize::from(slot)),
        _ => Err(TraceError("invalid observed guard slot".into())),
    }
}

fn facing(byte: u8) -> Facing {
    if byte & 0x80 == 0 {
        Facing::Right
    } else {
        Facing::Left
    }
}

fn map(bytes: &[u8]) -> Result<Vec<Tile>, TraceError> {
    if bytes.iter().any(|byte| *byte >= 10) {
        return Err(TraceError("invalid machine tile".into()));
    }
    Ok(bytes.iter().copied().map(Tile::decode).collect())
}

fn decimal(bytes: impl Iterator<Item = u8>) -> Result<u32, TraceError> {
    let mut result = 0;
    for byte in bytes {
        if byte >> 4 > 9 || byte & 15 > 9 {
            return Err(TraceError("invalid machine BCD score".into()));
        }
        result = result * 100 + u32::from(byte >> 4) * 10 + u32::from(byte & 15);
    }
    Ok(result)
}

fn decode_memory(hex: &str) -> Result<Box<[u8; 8192]>, TraceError> {
    if hex.len() != 16384 || !hex.is_ascii() {
        return Err(TraceError("invalid machine memory extent".into()));
    }
    let mut bytes = Box::new([0; 8192]);
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16)
            .map_err(|_| TraceError("invalid machine memory byte".into()))?;
    }
    Ok(bytes)
}
