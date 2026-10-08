//! Fixtures, word for word from CleatTests/Fixtures.swift.
#![allow(dead_code)]

pub mod engine;
pub mod reclaim;

use std::collections::BTreeMap;

use cleat_rs::config::{Config, LivenessConfig};
use cleat_rs::model::{
    AudioDevice, TRANSPORT_BLUETOOTH, TRANSPORT_BUILT_IN, TRANSPORT_DISPLAY_PORT, TRANSPORT_UNKNOWN, TRANSPORT_USB,
    TRANSPORT_VIRTUAL,
};

pub fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

pub fn volumes(v: &[(&str, f64)]) -> BTreeMap<String, f64> {
    v.iter().map(|(k, x)| (k.to_string(), *x)).collect()
}

pub fn wireless() -> AudioDevice {
    AudioDevice::with_transport(
        10,
        "Wireless microphone",
        "AppleUSBAudioEngine:Shenzhen Hollyland Technology Co.,Ltd:Wireless microphone:952X2D2Q952:3",
        true,
        false,
        TRANSPORT_USB,
    )
}
pub fn brio() -> AudioDevice {
    AudioDevice::with_transport(20, "Brio 100", "AppleUSBAudioEngine:Brio 100:B", true, false, TRANSPORT_USB)
}
pub fn second_brio() -> AudioDevice {
    AudioDevice::with_transport(21, "Brio 100", "AppleUSBAudioEngine:Brio 100:C", true, false, TRANSPORT_USB)
}
pub fn air_pods() -> AudioDevice {
    AudioDevice::with_transport(30, "AirPods Max", "AirPodsMax-UID", true, true, TRANSPORT_BLUETOOTH)
}
pub fn zoom() -> AudioDevice {
    AudioDevice::with_transport(40, "ZoomAudioDevice", "ZoomAudioDevice", true, true, TRANSPORT_VIRTUAL)
}
pub fn maono() -> AudioDevice {
    AudioDevice::with_transport(50, "Maono\u{00A0}AI Microphone", "Maono-UID", true, false, TRANSPORT_USB)
}
pub fn display_speakers() -> AudioDevice {
    AudioDevice::with_transport(60, "Studio Display Speakers", "StudioDisplay-UID", false, true, TRANSPORT_DISPLAY_PORT)
}
pub fn mac_speakers() -> AudioDevice {
    AudioDevice::with_transport(70, "MacBook Pro Speakers", "BuiltInSpeakerDevice", false, true, TRANSPORT_BUILT_IN)
}
pub fn mac_studio_speakers() -> AudioDevice {
    AudioDevice::with_transport(80, "Mac Studio的揚聲器", "BuiltInSpeakerDevice-MacStudio", false, true, TRANSPORT_BUILT_IN)
}
pub fn wired_headphones() -> AudioDevice {
    AudioDevice::with_transport(90, "外接耳機", "BuiltInHeadphoneOutputDevice", false, true, TRANSPORT_BUILT_IN)
}

pub fn mac_mic() -> AudioDevice {
    AudioDevice::with_transport(81, "MacBook Pro Microphone", "BuiltInMicrophoneDevice", true, false, TRANSPORT_BUILT_IN)
}
pub fn black_hole() -> AudioDevice {
    AudioDevice::with_transport(82, "BlackHole 2ch", "BlackHole2ch_UID", true, true, TRANSPORT_VIRTUAL)
}
pub fn iphone_mic() -> AudioDevice {
    AudioDevice::with_transport(83, "Albert\u{2019}s iPhone Microphone", "iPhone-UID", true, false, TRANSPORT_UNKNOWN)
}

/// The config Albert actually runs.
pub fn pinned_input() -> Config {
    Config {
        input: s(&["Wireless microphone", "Brio 100"]),
        blocked_input: s(&["AirPods Max"]),
        liveness: [("Wireless microphone".to_string(), LivenessConfig { zero_seconds: 3.0 })].into_iter().collect(),
        ..Config::default()
    }
}

/// Albert's real input list: AirPods Max is listed and blocked at once.
pub fn albert_input() -> Config {
    Config { input: s(&["Wireless microphone", "Brio 100", "AirPods Max"]), ..pinned_input() }
}
