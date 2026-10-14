use crate::event_timing::NoteWork;
use crate::timing::Clock;

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
        self.load_sound_data(&[note])
    }

    // Tune loading is atomic on overflow and preserves stack playback order.
    // The fixed capacity also bounds the note countdown's incoming low counter.
    pub fn load_sound_data(&mut self, notes: &[Note]) -> Result<(), String> {
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
