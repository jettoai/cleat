use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use super::device_name;

pub type AudioDeviceId = u32;

pub use objc2_core_audio::{
    kAudioDeviceTransportTypeBluetooth as TRANSPORT_BLUETOOTH,
    kAudioDeviceTransportTypeBluetoothLE as TRANSPORT_BLUETOOTH_LE,
    kAudioDeviceTransportTypeBuiltIn as TRANSPORT_BUILT_IN,
    kAudioDeviceTransportTypeDisplayPort as TRANSPORT_DISPLAY_PORT,
    kAudioDeviceTransportTypeUSB as TRANSPORT_USB,
    kAudioDeviceTransportTypeUnknown as TRANSPORT_UNKNOWN,
    kAudioDeviceTransportTypeVirtual as TRANSPORT_VIRTUAL,
};

/// One audio device as the rules see it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioDevice {
    pub id: AudioDeviceId,
    pub name: String,
    pub uid: String,
    pub has_input: bool,
    pub has_output: bool,
    /// `kAudioDevicePropertyTransportType`, unknown when the HAL will not answer.
    pub transport: u32,
}

impl AudioDevice {
    pub fn new(id: AudioDeviceId, name: &str, uid: &str, has_input: bool, has_output: bool) -> Self {
        Self::with_transport(id, name, uid, has_input, has_output, TRANSPORT_UNKNOWN)
    }

    pub fn with_transport(
        id: AudioDeviceId,
        name: &str,
        uid: &str,
        has_input: bool,
        has_output: bool,
        transport: u32,
    ) -> Self {
        Self { id, name: name.into(), uid: uid.into(), has_input, has_output, transport }
    }

    /// Classic and LE Bluetooth both count.
    pub fn is_bluetooth(&self) -> bool {
        self.transport == TRANSPORT_BLUETOOTH || self.transport == TRANSPORT_BLUETOOTH_LE
    }

    /// True when any of these config entries names this device.
    pub fn is_listed(&self, entries: &[String]) -> bool {
        entries.iter().any(|e| device_name::matches(e, &self.name, &self.uid))
    }

    /// By name, id breaking the tie.
    pub fn by_name(a: &AudioDevice, b: &AudioDevice) -> Ordering {
        a.name.cmp(&b.name).then(a.id.cmp(&b.id))
    }
}

/// What silence detection says about one device. A missing key means "not tracked".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Liveness {
    Measuring,
    Live,
    Silent,
}

/// Everything the rules are allowed to know: one reading of the audio system.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DeviceSnapshot {
    pub devices: Vec<AudioDevice>,
    pub default_input: Option<AudioDeviceId>,
    pub default_output: Option<AudioDeviceId>,
    /// Virtual main balance of the default output, 0.0 (left) - 1.0 (right).
    pub output_balance: Option<f32>,
    /// Whether something plays through the default output; read only when reclaim is on.
    pub output_running: bool,
    pub input_volumes: HashMap<AudioDeviceId, f32>,
    /// Silence verdict per device UID.
    pub liveness: HashMap<String, Liveness>,
    /// UIDs that appeared on this pass and were not here on the last one.
    pub arrived: HashSet<String>,
}

impl DeviceSnapshot {
    /// The present device a config entry names. `input` picks which side must exist.
    pub fn device_matching(&self, entry: &str, input: bool) -> Option<&AudioDevice> {
        self.devices.iter().find(|d| {
            (if input { d.has_input } else { d.has_output })
                && device_name::matches(entry, &d.name, &d.uid)
        })
    }

    pub fn device(&self, id: AudioDeviceId) -> Option<&AudioDevice> {
        self.devices.iter().find(|d| d.id == id)
    }
}
