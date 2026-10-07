//! Everything the engine may do to the audio system, behind a trait so tests can use a fake.

mod core_audio;
mod listeners;
pub(crate) mod property;

pub use core_audio::{prepare_liveness_input, CoreAudioSystem};
pub use listeners::{ListenerKind, ListenerToken, VIRTUAL_MAIN_BALANCE};

use crate::config::Config;
use crate::model::presence::PresenceFacts;
use crate::model::DeviceSnapshot;

/// What a listener is registered on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListenTarget {
    System(ListenerKind),
    Device { device: u32, kind: ListenerKind, element: u32 },
}

pub trait AudioSystem {
    /// One consistent reading. Input volumes are read only for the devices the config holds.
    fn snapshot(&self, config: &Config) -> DeviceSnapshot;
    fn set_default_input(&self, id: u32) -> i32;
    fn set_default_output(&self, id: u32) -> i32;
    fn set_balance(&self, id: u32, value: f32) -> i32;
    fn set_input_volume(&self, id: u32, value: f32) -> i32;
    /// Both channels to the same value (main element when settable).
    fn set_output_volume(&self, id: u32, value: f32) -> i32;
    /// What a silence detector's frame threshold is computed from.
    fn nominal_sample_rate(&self, id: u32) -> Option<f64>;
    fn add_listener(&self, target: ListenTarget) -> Option<ListenerToken>;
    fn remove_listener(&self, token: ListenerToken);
    /// Fills in the presence readings a snapshot leaves out because they cost more (assertions,
    /// the front app). Called only when reclaim is about to judge whether anyone is at the Mac.
    fn complete_presence(&self, _facts: &mut PresenceFacts) {}
    /// Whether this Mac holds the Bluetooth audio connection of `device`. `None` when the device
    /// has no such property or the HAL will not answer. Read only when an output change away from
    /// a listed headset looks manual.
    fn owns_bluetooth_audio(&self, _device: u32) -> Option<bool> {
        None
    }
}
