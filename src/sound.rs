use crate::timing::Clock;
use serde::{Deserialize, Serialize};

// The reference voice keeps the incoming counter phase because it affects the
// first note segment. This belongs to sound synthesis, not gameplay dispatch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteWork {
    pub enabled: bool,
    pub pitch: u8,
    pub duration: u8,
    pub low_counter: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Note {
    pub duration: u8,
    pub pitch: u8,
}

pub const DIG_NOTES: [Note; 13] = [
    Note {
        duration: 4,
        pitch: 32,
    },
    Note {
        duration: 4,
        pitch: 32,
    },
    Note {
        duration: 4,
        pitch: 32,
    },
    Note {
        duration: 4,
        pitch: 32,
    },
    Note {
        duration: 4,
        pitch: 32,
    },
    Note {
        duration: 4,
        pitch: 32,
    },
    Note {
        duration: 4,
        pitch: 32,
    },
    Note {
        duration: 4,
        pitch: 32,
    },
    Note {
        duration: 3,
        pitch: 36,
    },
    Note {
        duration: 3,
        pitch: 36,
    },
    Note {
        duration: 2,
        pitch: 36,
    },
    Note {
        duration: 2,
        pitch: 36,
    },
    Note {
        duration: 1,
        pitch: 36,
    },
];

pub struct Sound {
    notes: Vec<Note>,
    pub enabled: bool,
    pub frame_period: u8,
}

impl Default for Sound {
    fn default() -> Self {
        Self {
            notes: Vec::new(),
            enabled: true,
            frame_period: 6,
        }
    }
}

impl Sound {
    pub fn clear(&mut self) {
        self.notes.clear();
    }

    pub fn len(&self) -> usize {
        self.notes.len()
    }
    pub fn is_empty(&self) -> bool {
        self.notes.is_empty()
    }
    pub fn peek(&self) -> Option<Note> {
        self.notes.last().copied()
    }
    pub fn pop(&mut self) -> Option<Note> {
        self.notes.pop()
    }
    pub fn append_note(&mut self, note: Note) -> Result<(), String> {
        self.append_notes(&[note])
    }

    // Tune loading is atomic on overflow and preserves stack playback order.
    // The fixed capacity also bounds the note countdown's incoming low counter.
    pub fn append_notes(&mut self, notes: &[Note]) -> Result<(), String> {
        if self.notes.len() + notes.len() > 127 {
            return Err("sound buffer capacity exceeded".into());
        }
        self.notes.extend_from_slice(notes);
        Ok(())
    }
}

pub fn play_note(clock: &mut Clock, enabled: bool, pitch: u8, duration: u8, low_counter: u8) {
    clock.events.note(NoteWork {
        enabled,
        pitch,
        duration,
        low_counter,
    });
}

// Note work is evaluated without a second clock. Runs end only at speaker
// reloads or low-counter wraps, including the original zero-byte wrap case.
pub fn render_note(note: NoteWork, mut edge: impl FnMut(u64)) -> u64 {
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
