use crate::event_timing::NoteWork;
use crate::timing::Clock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Note {
    pub duration: u8,
    pub pitch: u8,
}

pub struct Sound {
    pub durations: [u8; 128],
    pub pitches: [u8; 128],
    pub index: u8,
    pub enabled: bool,
    pub frame_period: u8,
}

impl Default for Sound {
    fn default() -> Self {
        Self {
            durations: [0; 128],
            pitches: [0; 128],
            index: 0,
            enabled: true,
            frame_period: 6,
        }
    }
}

impl Sound {
    pub fn append_note(&mut self, note: Note) -> Result<(), String> {
        let index = usize::from(self.index) + 1;
        if index >= self.durations.len() {
            return Err("original sound buffer capacity exceeded".into());
        }
        self.durations[index] = note.duration;
        self.pitches[index] = note.pitch;
        self.index += 1;
        Ok(())
    }

    pub fn load_sound_data(&mut self, notes: &[Note]) -> Result<(), String> {
        if usize::from(self.index) + notes.len() >= self.durations.len() {
            return Err("original sound buffer capacity exceeded".into());
        }
        for &note in notes {
            self.append_note(note)?;
        }
        Ok(())
    }
}

// Note playback records the original arguments. The interval model schedules
// speaker edges after gameplay has completed, including muted countdown work.
pub fn play_note(clock: &mut Clock, enabled: bool, pitch: u8, duration: u8, low_counter: u8) {
    clock.events.note(NoteWork {
        enabled,
        pitch,
        duration,
        low_counter,
    });
}
