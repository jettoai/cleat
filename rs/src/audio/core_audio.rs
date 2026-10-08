//! The real HAL. Balance goes through the virtual main balance, which posts change notifications.

use std::sync::mpsc::Sender;

use objc2_core_audio::{
    kAudioDevicePropertyBufferFrameSize, kAudioDevicePropertyBufferFrameSizeRange,
    kAudioDevicePropertyDeviceIsRunningSomewhere, kAudioDevicePropertyNominalSampleRate, kAudioDevicePropertyDeviceUID, kAudioDevicePropertyTransportType,
    kAudioDevicePropertyVolumeScalar, kAudioDeviceTransportTypeUnknown, kAudioHardwarePropertyDefaultInputDevice,
    kAudioHardwarePropertyDefaultOutputDevice, kAudioHardwareUnknownPropertyError, kAudioObjectPropertyElementMain,
    kAudioObjectPropertyName, kAudioObjectPropertyScopeInput, kAudioObjectPropertyScopeOutput, kAudioObjectUnknown,
    AudioObjectPropertyAddress, AudioObjectPropertySelector,
};

use objc2_core_audio_types::{AudioStreamBasicDescription, AudioValueRange};

use super::listeners::{Contexts, VIRTUAL_MAIN_BALANCE};
use super::property::{self, address, global, SYSTEM_OBJECT};
use super::{AudioSystem, ListenTarget, ListenerToken};
use crate::config::Config;
use crate::engine::Event;
use crate::app::presence;
use crate::model::presence::{PresenceFacts, USER_ACTIVE_WINDOW_S};
use crate::model::{AudioDevice, DeviceSnapshot};
use crate::rules::output_volume_hold::level;

pub struct CoreAudioSystem {
    contexts: Contexts,
}

fn balance_address() -> AudioObjectPropertyAddress {
    address(VIRTUAL_MAIN_BALANCE, kAudioObjectPropertyScopeOutput, kAudioObjectPropertyElementMain)
}

fn volume_address(element: u32) -> AudioObjectPropertyAddress {
    address(kAudioDevicePropertyVolumeScalar, kAudioObjectPropertyScopeInput, element)
}

fn output_volume_address(element: u32) -> AudioObjectPropertyAddress {
    address(kAudioDevicePropertyVolumeScalar, kAudioObjectPropertyScopeOutput, element)
}

/// Main element alone when it answers, otherwise channels 1 and 2 (Swift `volumes`).
fn volumes(id: u32, addr: fn(u32) -> AudioObjectPropertyAddress) -> Vec<f32> {
    if let Some(v) = property::get::<f32>(id, addr(kAudioObjectPropertyElementMain)) {
        return vec![v];
    }
    [1, 2].iter().filter_map(|&e| property::get::<f32>(id, addr(e))).collect()
}

/// Main element when settable, otherwise each settable channel.
fn set_volume(id: u32, value: f32, addr: fn(u32) -> AudioObjectPropertyAddress) -> i32 {
    let main = addr(kAudioObjectPropertyElementMain);
    if property::is_settable(id, main) {
        return property::set(id, main, value);
    }
    let mut last_error = kAudioHardwareUnknownPropertyError;
    let mut wrote_one = false;
    for channel in [1, 2] {
        let a = addr(channel);
        if !property::is_settable(id, a) {
            continue;
        }
        match property::set(id, a, value) {
            0 => wrote_one = true,
            status => last_error = status,
        }
    }
    if wrote_one { 0 } else { last_error }
}

impl CoreAudioSystem {
    pub fn new(tx: &Sender<Event>) -> Self {
        Self { contexts: Contexts::new(tx) }
    }

    pub(crate) fn describe(id: u32) -> Option<AudioDevice> {
        let name = property::get_string(id, global(kAudioObjectPropertyName))?;
        let uid = property::get_string(id, global(kAudioDevicePropertyDeviceUID))?;
        Some(AudioDevice::with_transport(
            id,
            &name,
            &uid,
            property::channel_count(id, kAudioObjectPropertyScopeInput) > 0,
            property::channel_count(id, kAudioObjectPropertyScopeOutput) > 0,
            property::get::<u32>(id, global(kAudioDevicePropertyTransportType)).unwrap_or(kAudioDeviceTransportTypeUnknown),
        ))
    }

    pub(crate) fn default_device(selector: AudioObjectPropertySelector) -> Option<u32> {
        property::get::<u32>(SYSTEM_OBJECT, global(selector)).filter(|&id| id != kAudioObjectUnknown)
    }

    fn is_running_somewhere(id: u32) -> bool {
        property::get::<u32>(id, global(kAudioDevicePropertyDeviceIsRunningSomewhere)).unwrap_or(0) != 0
    }

    /// Main element first; otherwise the mean of channels 1 and 2.
    fn input_volume(id: u32) -> Option<f32> {
        level(&volumes(id, volume_address))
    }
}

/// Frames per IOProc callback a detector asks for; 4096 is the ceiling most devices allow.
const PREFERRED_BUFFER_FRAMES: u32 = 4096;

/// Readies an input for a silence detector (Swift `applyPreferredBufferSize`, `readStreamFormat`):
/// the largest buffer up to 4096 frames, then the sample width buffers will arrive in. Float32,
/// the HAL's virtual format, when the device will not say.
pub fn prepare_liveness_input(id: u32) -> usize {
    let range = address(kAudioDevicePropertyBufferFrameSizeRange, kAudioObjectPropertyScopeInput, kAudioObjectPropertyElementMain);
    let mut wanted = PREFERRED_BUFFER_FRAMES;
    if let Some(r) = property::get::<AudioValueRange>(id, range) {
        wanted = wanted.max(r.mMinimum as u32).min(r.mMaximum as u32);
    }
    let size = address(kAudioDevicePropertyBufferFrameSize, kAudioObjectPropertyScopeInput, kAudioObjectPropertyElementMain);
    let _ = property::set(id, size, wanted);
    let format = address(STREAM_FORMAT, kAudioObjectPropertyScopeInput, kAudioObjectPropertyElementMain);
    match property::get::<AudioStreamBasicDescription>(id, format) {
        Some(f) if f.mBitsPerChannel >= 8 => (f.mBitsPerChannel / 8) as usize,
        _ => std::mem::size_of::<f32>(),
    }
}

/// `kAudioDevicePropertyStreamFormat` ('sfmt', AudioHardwareDeprecated.h:694), which the crate
/// only exports behind its deprecated-API feature.
const STREAM_FORMAT: u32 = u32::from_be_bytes(*b"sfmt");

/// `kBluetoothAudioDevicePropertyOwnsAudioConnection`, a UInt32 the Bluetooth HAL plug-in keeps on
/// a Bluetooth output device: 1 while this Mac holds the headset, 0 once another device (an iPhone)
/// took it. Not in any public header; read off BTAudioHALPlugin's property dispatch on 2026-10-07
/// (macOS 27, `cmp 0x626f6163` ahead of "Ownership Set as %d"). Other devices answer 'who?'.
const BT_OWNS_AUDIO_CONNECTION: u32 = u32::from_be_bytes(*b"boac");

impl AudioSystem for CoreAudioSystem {
    fn snapshot(&self, config: &Config) -> DeviceSnapshot {
        let devices: Vec<AudioDevice> = property::device_ids().into_iter().filter_map(Self::describe).collect();
        let default_input = Self::default_device(kAudioHardwarePropertyDefaultInputDevice);
        let default_output = Self::default_device(kAudioHardwarePropertyDefaultOutputDevice);
        let input_volumes = config
            .input_volume_devices(&devices)
            .iter()
            .filter_map(|d| Self::input_volume(d.id).map(|v| (d.id, v)))
            .collect();
        let output_running = !config.reclaim_active().is_empty() && default_output.is_some_and(Self::is_running_somewhere);
        // Only reclaim asks who is at the Mac, and only while it is playing. The rest of the
        // reading (assertions, front app) waits until the rule actually asks: `complete_presence`.
        let presence = PresenceFacts {
            input_idle: if output_running { presence::input_idle_seconds() } else { None },
            ..Default::default()
        };
        let output_balance = default_output.and_then(|id| property::get::<f32>(id, balance_address()));
        DeviceSnapshot {
            devices,
            default_input,
            default_output,
            output_balance,
            output_running,
            input_volumes,
            presence,
            // Only the output volume hold reads the output volume.
            output_volumes: if config.hold_against_active().is_empty() {
                vec![]
            } else {
                default_output.map(|id| volumes(id, output_volume_address)).unwrap_or_default()
            },
            ..Default::default()
        }
    }

    fn complete_presence(&self, facts: &mut PresenceFacts) {
        if facts.input_idle.is_some_and(|s| s < USER_ACTIVE_WINDOW_S) {
            return;
        }
        let full = presence::read_assertions_and_front();
        facts.display_assertions = full.display_assertions;
        facts.front_pid = full.front_pid;
    }

    fn owns_bluetooth_audio(&self, device: u32) -> Option<bool> {
        property::get::<u32>(device, global(BT_OWNS_AUDIO_CONNECTION)).map(|v| v != 0)
    }

    fn set_default_input(&self, id: u32) -> i32 {
        property::set(SYSTEM_OBJECT, global(kAudioHardwarePropertyDefaultInputDevice), id)
    }

    fn set_default_output(&self, id: u32) -> i32 {
        property::set(SYSTEM_OBJECT, global(kAudioHardwarePropertyDefaultOutputDevice), id)
    }

    fn set_balance(&self, id: u32, value: f32) -> i32 {
        property::set(id, balance_address(), value)
    }

    fn set_input_volume(&self, id: u32, value: f32) -> i32 {
        set_volume(id, value, volume_address)
    }

    fn set_output_volume(&self, id: u32, value: f32) -> i32 {
        set_volume(id, value, output_volume_address)
    }

    fn nominal_sample_rate(&self, id: u32) -> Option<f64> {
        property::get::<f64>(id, global(kAudioDevicePropertyNominalSampleRate))
    }

    fn add_listener(&self, target: ListenTarget) -> Option<ListenerToken> {
        self.contexts.add(target)
    }

    fn remove_listener(&self, token: ListenerToken) {
        self.contexts.remove(token)
    }
}
