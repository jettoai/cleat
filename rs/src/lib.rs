//! Cleat, ported to Rust: rules, CoreAudio reads and writes, the event loop, the CLI and the
//! launchd agent. Observe mode (`run --observe`) decides but never writes.

pub mod audio;
pub mod cli;
pub mod config;
pub mod engine;
pub mod identity;
pub mod launch;
pub mod model;
pub mod rules;
pub mod state;
