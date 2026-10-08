//! Fixtures, word for word from CleatTests/Fixtures.swift.
#![allow(dead_code)]

pub mod engine;
pub mod reclaim;

use std::collections::BTreeMap;

use cleat_rs::config::{Config, LivenessConfig};
use cleat_rs::model::{
    AudioDevice, TRANSPORT_BLUETOOTH, TRANSPORT_BUILT_IN, TRANSPORT_CONTINUITY_WIRELESS, TRANSPORT_DISPLAY_PORT,
    TRANSPORT_HDMI, TRANSPORT_UNKNOWN, TRANSPORT_USB, TRANSPORT_VIRTUAL,
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

// Albert's Mac Studio (Mac16,9), from `system_profiler` on 2026-10-09 (B-1287, PM 0209 #2).

/// The Maono as it really shows up: one device with a microphone and a speaker side.
pub fn maono_io() -> AudioDevice {
    AudioDevice::with_transport(93, "Maono AI Microphone", "Maono-IO-UID", true, true, TRANSPORT_USB)
}
pub fn teams() -> AudioDevice {
    AudioDevice::with_transport(92, "Microsoft Teams Audio", "MSTeamsAudioDevice_UID", true, true, TRANSPORT_VIRTUAL)
}
pub fn dell() -> AudioDevice {
    AudioDevice::with_transport(95, "DELL U3223QE", "DELL-UID", false, true, TRANSPORT_DISPLAY_PORT)
}
pub fn xv272u() -> AudioDevice {
    AudioDevice::with_transport(96, "XV272U", "XV272U-UID", false, true, TRANSPORT_HDMI)
}
pub fn continuity_iphone() -> AudioDevice {
    AudioDevice::with_transport(
        85,
        "Albert\u{2019}s iPhone Microphone",
        "iPhone-CC-UID",
        true,
        false,
        TRANSPORT_CONTINUITY_WIRELESS,
    )
}

/// No built-in microphone; the built-in outputs are the jack and the speakers he blocks. `jack`
/// false = nothing plugged into the 3.5 mm port.
pub fn mac_studio(jack: bool, air_pods_here: bool, wireless_here: bool, brio_here: bool) -> Vec<AudioDevice> {
    let mut v = vec![mac_studio_speakers(), dell(), xv272u(), maono_io(), teams(), zoom()];
    if jack {
        v.push(wired_headphones());
    }
    if air_pods_here {
        v.push(air_pods());
    }
    if wireless_here {
        v.push(wireless());
    }
    if brio_here {
        v.push(brio());
    }
    v
}

/// ~/.config/cleat/config.json as of 2026-10-09.
pub fn albert_config() -> Config {
    Config {
        input: s(&["Wireless microphone", "Brio 100", "AirPods Max"]),
        blocked_input: s(&["AirPods Max", "Microsoft Teams Audio", "ZoomAudioDevice"]),
        output: s(&["外接耳機"]),
        blocked_output: s(&["Mac Studio的揚聲器", "Maono AI Microphone"]),
        headphones_take_over: true,
        balance: Some(0.5),
        liveness: [("Wireless microphone".to_string(), LivenessConfig { zero_seconds: 3.0 })].into_iter().collect(),
        reclaim: s(&["AirPods Max", "AirPods Pro"]),
        launch_at_login: false,
        ..Config::default()
    }
}
