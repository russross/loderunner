use crate::devices::speaker::Speaker;
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

// PLAY_NOTE starts its pitch countdown with the duration argument. Subsequent
// countdowns use pitch. The incoming low counter is intentionally not reset.
pub fn play_note(
    clock: &mut Clock,
    speaker: &mut Speaker,
    enabled: bool,
    pitch: u8,
    duration: u8,
    low_counter: u8,
) {
    let count = |byte: u8| if byte == 0 { 256_u32 } else { u32::from(byte) };
    let mut remaining = (count(duration) - 1) * 256 + count(low_counter);
    let mut until_wrap = count(low_counter);
    let mut pitch_counter = count(duration);
    clock.advance(6);

    // Each iteration consumes ten cycles, with six extra cycles on a nonterminal
    // low-counter wrap and five extra cycles when the pitch counter reloads.
    while remaining > 0 {
        clock.advance(if enabled { 9 } else { 6 });
        if enabled {
            speaker.access(clock.cycles);
        }
        loop {
            let steps = pitch_counter.min(until_wrap).min(remaining);
            remaining -= steps;
            until_wrap -= steps;
            pitch_counter -= steps;
            if remaining == 0 {
                clock.advance(u64::from(steps - 1) * 10 + 18);
                return;
            }
            clock.advance(u64::from(steps) * 10);
            if until_wrap == 0 {
                clock.advance(6);
                until_wrap = 256;
            }
            if pitch_counter == 0 {
                clock.advance(5);
                pitch_counter = count(pitch);
                break;
            }
        }
    }
}
