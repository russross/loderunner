//! Input recordings carry configuration and consumed keys, without gameplay state.

use crate::session::Configuration;
use lode_trace::Input;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recording {
    version: u8,
    pub configuration: Configuration,
    pub rounds: usize,
    inputs: BTreeMap<usize, u8>,
}

impl Recording {
    pub fn new(configuration: Configuration) -> Self {
        Self {
            version: 2,
            configuration,
            rounds: 0,
            inputs: BTreeMap::new(),
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
