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
use crate::model::{AudioDevice, DeviceSnapshot};

pub struct CoreAudioSystem {
    contexts: Contexts,
}

fn balance_address() -> AudioObjectPropertyAddress {
    address(VIRTUAL_MAIN_BALANCE, kAudioObjectPropertyScopeOutput, kAudioObjectPropertyElementMain)
}

fn volume_address(element: u32) -> AudioObjectPropertyAddress {
    address(kAudioDevicePropertyVolumeScalar, kAudioObjectPropertyScopeInput, element)
}

impl CoreAudioSystem {
    pub fn new(tx: &Sender<Event>) -> Self {
        Self { contexts: Contexts::new(tx) }
    }

    fn describe(id: u32) -> Option<AudioDevice> {
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

    fn default_device(selector: AudioObjectPropertySelector) -> Option<u32> {
        property::get::<u32>(SYSTEM_OBJECT, global(selector)).filter(|&id| id != kAudioObjectUnknown)
    }

    fn is_running_somewhere(id: u32) -> bool {
        property::get::<u32>(id, global(kAudioDevicePropertyDeviceIsRunningSomewhere)).unwrap_or(0) != 0
    }

    /// Main element first; otherwise the mean of channels 1 and 2.
    fn input_volume(id: u32) -> Option<f32> {
        if let Some(v) = property::get::<f32>(id, volume_address(kAudioObjectPropertyElementMain)) {
            return Some(v);
        }
        let channels: Vec<f32> = [1, 2].iter().filter_map(|&e| property::get::<f32>(id, volume_address(e))).collect();
        if channels.is_empty() {
            return None;
        }
        Some(channels.iter().sum::<f32>() / channels.len() as f32)
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
        let output_running = !config.reclaim.is_empty() && default_output.is_some_and(Self::is_running_somewhere);
        let output_balance = default_output.and_then(|id| property::get::<f32>(id, balance_address()));
        DeviceSnapshot {
            devices,
            default_input,
            default_output,
            output_balance,
            output_running,
            input_volumes,
            ..Default::default()
        }
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

    /// Main element when settable, otherwise each settable channel.
    fn set_input_volume(&self, id: u32, value: f32) -> i32 {
        let main = volume_address(kAudioObjectPropertyElementMain);
        if property::is_settable(id, main) {
            return property::set(id, main, value);
        }
        let mut last_error = kAudioHardwareUnknownPropertyError;
        let mut wrote_one = false;
        for channel in [1, 2] {
            let addr = volume_address(channel);
            if !property::is_settable(id, addr) {
                continue;
            }
            match property::set(id, addr, value) {
                0 => wrote_one = true,
                status => last_error = status,
            }
        }
        if wrote_one { 0 } else { last_error }
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
