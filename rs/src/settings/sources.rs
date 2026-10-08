//! What the window reads from the system: present devices and live levels from CoreAudio, the
//! pairing list from system_profiler, and two fields of the daemon's status.json.

use objc2_core_audio::{
    kAudioDevicePropertyVolumeScalar, kAudioHardwarePropertyDefaultInputDevice,
    kAudioHardwarePropertyDefaultOutputDevice, kAudioObjectPropertyElementMain, kAudioObjectPropertyName,
    kAudioObjectPropertyScopeInput, kAudioObjectPropertyScopeOutput, AudioObjectPropertyScope,
};
use serde_json::Value;

use super::draft::{LiveLevels, PairedDevice};
use crate::audio::property::{self, address, global};
use crate::audio::{CoreAudioSystem, VIRTUAL_MAIN_BALANCE};
use crate::model::AudioDevice;
use crate::rules::output_volume_hold::level;
use crate::state::reaction_clock::DaemonPerformance;
use crate::reclaim::pairings;

/// `kAudioHardwareServiceDeviceProperty_VirtualMainVolume` ('vmvc').
const VIRTUAL_MAIN_VOLUME: u32 = u32::from_be_bytes(*b"vmvc");

pub fn present_devices() -> Vec<AudioDevice> {
    property::device_ids().into_iter().filter_map(CoreAudioSystem::describe).collect()
}

/// Virtual main volume first, then the main element, then the mean of channels 1 and 2.
fn volume(id: u32, scope: AudioObjectPropertyScope) -> Option<f32> {
    if let Some(v) = property::get::<f32>(id, address(VIRTUAL_MAIN_VOLUME, scope, kAudioObjectPropertyElementMain)) {
        return Some(v);
    }
    let scalar = |e: u32| property::get::<f32>(id, address(kAudioDevicePropertyVolumeScalar, scope, e));
    if let Some(v) = scalar(kAudioObjectPropertyElementMain) {
        return Some(v);
    }
    level(&[1, 2].into_iter().filter_map(scalar).collect::<Vec<_>>())
}

pub fn live_levels() -> LiveLevels {
    let mut l = LiveLevels::default();
    if let Some(out) = CoreAudioSystem::default_device(kAudioHardwarePropertyDefaultOutputDevice) {
        l.output_device = property::get_string(out, global(kAudioObjectPropertyName));
        l.output_volume = volume(out, kAudioObjectPropertyScopeOutput).map(|v| v as f64 * 100.0);
        l.balance = property::get::<f32>(out, address(VIRTUAL_MAIN_BALANCE, kAudioObjectPropertyScopeOutput, kAudioObjectPropertyElementMain))
            .map(f64::from);
    }
    if let Some(inp) = CoreAudioSystem::default_device(kAudioHardwarePropertyDefaultInputDevice) {
        l.input_device = property::get_string(inp, global(kAudioObjectPropertyName));
        l.input_volume = volume(inp, kAudioObjectPropertyScopeInput).map(|v| v as f64 * 100.0);
    }
    l
}

pub fn paired_devices() -> Vec<PairedDevice> {
    pairings::profiler_output().map(|d| parse_paired(&d)).unwrap_or_default()
}

/// `reclaim::pairings::parse` plus `device_minorType`.
pub fn parse_paired(data: &[u8]) -> Vec<PairedDevice> {
    pairings::parse_with(data, |name, address, is_connected, fields| PairedDevice {
        name,
        address,
        is_connected,
        minor_type: fields.get("device_minorType").and_then(Value::as_str).map(String::from),
    })
}

/// status.json's `outputVolume.lastRevert`; None when the file, section or field is missing.
pub fn last_revert(data: &[u8]) -> Option<String> {
    let v: Value = serde_json::from_slice(data).ok()?;
    v.get("outputVolume")?.get("lastRevert")?.as_str().map(String::from)
}

/// status.json `performance`; None for a file written before the measurement existed.
pub fn performance(data: &[u8]) -> Option<DaemonPerformance> {
    let v: Value = serde_json::from_slice(data).ok()?;
    serde_json::from_value(v.get("performance")?.clone()).ok()
}

pub fn status_pid(data: &[u8]) -> Option<i32> {
    let v: Value = serde_json::from_slice(data).ok()?;
    v.get("pid")?.as_i64().map(|p| p as i32)
}
