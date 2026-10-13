//! Procedural translation of the Apple II game and its device boundary.

pub mod devices;
pub mod graphics;
pub mod guards;
pub mod player;
pub mod rounds;
pub mod score;
pub mod sound;
pub mod state;
pub mod storage;
pub mod timing;

pub static IMAGE: &[u8] = include_bytes!("../../sources/lode_runner_reveng/main.bin");
