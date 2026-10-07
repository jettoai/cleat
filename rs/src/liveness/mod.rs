//! Silence detection for configured input devices (Swift `Core/Liveness`). The engine owns one
//! detector per present device and gets a `LivenessFlip` event when the verdict changes.

mod detector;
pub mod ioproc;
pub mod streak;

pub use detector::{flip_channel, ZeroSignalDetector};
pub use streak::{Flip, ZeroStreak};

use std::sync::mpsc::Sender;

use crate::engine::Event;
use crate::model::AudioDevice;

/// What the engine needs from a detector, so its bookkeeping can be tested without a device.
pub trait LivenessDetecting {
    fn device_id(&self) -> u32;
    fn name(&self) -> &str;
    /// Baked in at construction; a config change to it needs a new detector.
    fn zero_seconds(&self) -> f64;
    /// False when the input could not be opened.
    fn start(&mut self) -> bool;
    fn stop(&mut self);
}

/// Device, nominal sample rate, zero seconds.
pub type DetectorFactory = Box<dyn Fn(&AudioDevice, f64, f64) -> Box<dyn LivenessDetecting>>;

/// The factory the daemon uses: real IOProcs, flips delivered on `tx`.
pub fn live_detectors(tx: Sender<Event>) -> DetectorFactory {
    Box::new(move |device, sample_rate, zero_seconds| {
        Box::new(ZeroSignalDetector::new(device, sample_rate, zero_seconds, tx.clone()))
    })
}
