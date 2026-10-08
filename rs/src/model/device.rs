use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use super::device_name;
use super::presence::PresenceFacts;

pub type AudioDeviceId = u32;

pub use objc2_core_audio::{
    kAudioDeviceTransportTypeAggregate as TRANSPORT_AGGREGATE,
    kAudioDeviceTransportTypeAirPlay as TRANSPORT_AIRPLAY,
    kAudioDeviceTransportTypeBluetooth as TRANSPORT_BLUETOOTH,
    kAudioDeviceTransportTypeBluetoothLE as TRANSPORT_BLUETOOTH_LE,
    kAudioDeviceTransportTypeBuiltIn as TRANSPORT_BUILT_IN,
    kAudioDeviceTransportTypeContinuityCaptureWired as TRANSPORT_CONTINUITY_WIRED,
    kAudioDeviceTransportTypeContinuityCaptureWireless as TRANSPORT_CONTINUITY_WIRELESS,
    kAudioDeviceTransportTypeDisplayPort as TRANSPORT_DISPLAY_PORT,
    kAudioDeviceTransportTypeHDMI as TRANSPORT_HDMI,
    kAudioDeviceTransportTypeUSB as TRANSPORT_USB,
    kAudioDeviceTransportTypeUnknown as TRANSPORT_UNKNOWN,
    kAudioDeviceTransportTypeVirtual as TRANSPORT_VIRTUAL,
};

/// `'fgrp'`, the HAL's own aggregate; only in the crate's deprecated header set.
pub const TRANSPORT_AUTO_AGGREGATE: u32 = 0x6667_7270;
/// `'ccap'`, Continuity Capture before Apple split it into wired and wireless (deprecated).
pub const TRANSPORT_CONTINUITY: u32 = 0x6363_6170;

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

    /// The Mac's own microphone, speakers or headphone jack, by `kAudioDevicePropertyTransportType`.
    pub fn is_built_in(&self) -> bool {
        self.transport == TRANSPORT_BUILT_IN
    }

    /// A device sound actually comes out of or into: not a virtual or aggregate one, not an
    /// iPhone over Continuity, not an AirPlay target, and not one whose transport the HAL would
    /// not name. The only pool the "not used" rule may move an output into (PM, B-1287).
    pub fn is_physical(&self) -> bool {
        !matches!(
            self.transport,
            TRANSPORT_VIRTUAL
                | TRANSPORT_AGGREGATE
                | TRANSPORT_AUTO_AGGREGATE
                | TRANSPORT_AIRPLAY
                | TRANSPORT_CONTINUITY
                | TRANSPORT_CONTINUITY_WIRED
                | TRANSPORT_CONTINUITY_WIRELESS
                | TRANSPORT_UNKNOWN
        )
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
    /// The default output's volume per channel (main element alone when it has one); read only
    /// when the output volume hold is on.
    pub output_volumes: Vec<f32>,
    /// Whether something plays through the default output; read only when reclaim is on.
    pub output_running: bool,
    pub input_volumes: HashMap<AudioDeviceId, f32>,
    /// Silence verdict per device UID.
    pub liveness: HashMap<String, Liveness>,
    /// UIDs that appeared on this pass and were not here on the last one.
    pub arrived: HashSet<String>,
    /// Who is at the Mac; read only while reclaim is on and the Mac is playing.
    pub presence: PresenceFacts,
    /// UID of the input cleat itself moved to when it evicted a blocked one. Filled by the engine,
    /// like `liveness`: that device is cleat's choice, not the user's, so a listed device that comes
    /// back takes over from it (B-1283).
    pub placed_input: Option<String>,
    /// The same for the output.
    pub placed_output: Option<String>,
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

    /// The default output, when it is among the present devices.
    pub fn output_device(&self) -> Option<&AudioDevice> {
        self.default_output.and_then(|id| self.device(id))
    }
}
