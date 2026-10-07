//! The rules: pure functions of a `DeviceSnapshot` and a `Config`, returning actions. No CoreAudio,
//! no engine, no I/O.

pub mod balance;
pub mod headphones_takeover;
pub mod input_pin;
pub mod input_volume;
pub mod output_pin;
pub mod reclaim;
