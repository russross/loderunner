//! Input recordings carry configuration and consumed keys, without gameplay state.

use crate::session::Configuration;
use lode_trace::Input;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::num::NonZeroU8;

// Columns are validated at the recording boundary before reaching board indexing.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct ResurrectionColumn(u8);

impl TryFrom<u8> for ResurrectionColumn {
    type Error = String;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        if value < 28 {
            Ok(Self(value))
        } else {
            Err("resurrection column must be between 0 and 27".into())
        }
    }
}

impl From<ResurrectionColumn> for u8 {
    fn from(value: ResurrectionColumn) -> Self {
        value.0
    }
}

// Entries address occurrences, independently of frame timing and guard identity.
// Null entries and exhausted lists defer to the ordinary counter-based rules.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Overrides {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gold_delays: Vec<Option<NonZeroU8>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resurrection_columns: Vec<Option<ResurrectionColumn>>,
}

impl Overrides {
    pub fn is_empty(&self) -> bool {
        self.gold_delays.is_empty() && self.resurrection_columns.is_empty()
    }
}

// Playback cursors survive board reloads just as the input recording does.
#[derive(Default)]
pub struct OverridePlayback {
    pub values: Overrides,
    gold_pickups: usize,
    burials: usize,
}

impl OverridePlayback {
    pub fn new(values: Overrides) -> Self {
        Self {
            values,
            ..Self::default()
        }
    }

    pub fn gold_delay(&mut self, default: NonZeroU8) -> NonZeroU8 {
        let value = self
            .values
            .gold_delays
            .get(self.gold_pickups)
            .copied()
            .flatten();
        self.gold_pickups += 1;
        value.unwrap_or(default)
    }

    pub fn resurrection_column(&mut self) -> Option<u8> {
        let value = self
            .values
            .resurrection_columns
            .get(self.burials)
            .copied()
            .flatten();
        self.burials += 1;
        value.map(u8::from)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recording {
    version: u8,
    pub configuration: Configuration,
    pub rounds: usize,
    inputs: BTreeMap<usize, u8>,
    #[serde(default, skip_serializing_if = "Overrides::is_empty")]
    pub overrides: Overrides,
}

impl Recording {
    pub fn new(configuration: Configuration) -> Self {
        Self {
            version: 2,
            configuration,
            rounds: 0,
            inputs: BTreeMap::new(),
            overrides: Overrides::default(),
        }
    }

    // Repeated keys remain separate observations; silence needs no stored entry.
    pub fn push(&mut self, inputs: &[Input]) -> Result<(), String> {
        let key = observed_key(inputs)?;
        if let Some(key) = key {
            self.inputs.insert(self.rounds, key);
        }
        self.rounds = self.rounds.checked_add(1).ok_or("recording is too long")?;
        Ok(())
    }

    pub fn key(&self, round: usize) -> Option<u8> {
        self.inputs.get(&round).copied()
    }

    // Parsing rejects unsupported formats and input outside the explicit endpoint.
    pub fn from_json(text: &str) -> Result<Self, String> {
        let recording: Self = serde_json::from_str(text).map_err(|error| error.to_string())?;
        if recording.version != 2 {
            return Err("unsupported input recording version".into());
        }
        if recording
            .inputs
            .keys()
            .any(|&round| round >= recording.rounds)
        {
            return Err("input outside recording".into());
        }
        if recording.inputs.values().any(|&key| key < 0x80) {
            return Err("input codes must have the high bit set".into());
        }
        Ok(recording)
    }

    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|error| error.to_string())
    }
}

pub fn observed_key(inputs: &[Input]) -> Result<Option<u8>, String> {
    match inputs {
        [] => Ok(None),
        [input] if input.observation == 0 && input.key >= 0x80 => Ok(Some(input.key)),
        _ => Err("unsupported input observation".into()),
    }
}
