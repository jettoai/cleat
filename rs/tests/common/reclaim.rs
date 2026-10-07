//! The reclaim world (Swift `ReclaimTestDoubles.swift`): an engine playing through the Mac
//! Studio's speakers with the AirPods absent from CoreAudio, every outside edge faked. Someone is
//! at the keyboard unless a test says otherwise: Swift's `inputIdle: nil` meant that, and B-1173
//! made an unreadable idle count as nobody.

use std::time::Duration;

use super::engine::{Harness, Opts};
use super::{mac_studio_speakers, s};
use cleat_rs::config::Config;
use cleat_rs::model::presence::PresenceFacts;
use cleat_rs::model::{BluetoothHeadset, DeviceSnapshot};
use cleat_rs::reclaim::RouteResponse;

pub const AIR_PODS_ADDRESS: &str = "70:F9:4A:B6:0C:C9";

pub fn headset(name: &str, address: &str, connected: bool) -> BluetoothHeadset {
    BluetoothHeadset { name: name.into(), address: address.into(), is_connected: connected }
}
pub fn connected_air_pods() -> BluetoothHeadset {
    headset("AirPods Max", AIR_PODS_ADDRESS, true)
}
pub fn connected_air_pods_pro() -> BluetoothHeadset {
    headset("AirPods Pro", "7C:F3:4D:68:72:76", true)
}

pub fn reclaim_config(list: &[&str]) -> Config {
    Config { reclaim: s(list), launch_at_login: false, ..Config::default() }
}

pub fn at_keyboard() -> PresenceFacts {
    PresenceFacts { input_idle: Some(1.0), ..Default::default() }
}

pub fn playing() -> DeviceSnapshot {
    let speakers = mac_studio_speakers();
    DeviceSnapshot {
        default_output: Some(speakers.id),
        devices: vec![speakers],
        output_running: true,
        presence: at_keyboard(),
        ..Default::default()
    }
}

pub fn secs(n: f64) -> Duration {
    Duration::from_secs_f64(n)
}

pub fn answer(action: i64, reason: &str) -> RouteResponse {
    RouteResponse { action: Some(action), reason: Some(reason.into()), error: None }
}
pub fn hijacked() -> RouteResponse {
    answer(1, "Tipi device hijack was successful")
}
pub fn already_routed() -> RouteResponse {
    answer(1, "Device already routed")
}
pub fn out_of_ear() -> RouteResponse {
    answer(0, "Buds out of ear")
}
pub fn on_a_call() -> RouteResponse {
    answer(0, "Rejected, Remote Category 501 > Local Category 201, phone call")
}
pub fn media_hold() -> RouteResponse {
    answer(0, "Rejected, Remote Category 301 > Local Category 200, audio streaming")
}

/// A started world. `snap` replaces the default playing snapshot.
pub fn world_with(config: Config, answer: Option<RouteResponse>, headsets: Vec<BluetoothHeadset>, snap: DeviceSnapshot) -> Harness {
    Harness::new(&config, snap, Opts { route_answer: answer, headsets, ..Opts::default() })
}

pub fn world(answer: Option<RouteResponse>) -> Harness {
    world_with(reclaim_config(&["AirPods Max"]), answer, vec![connected_air_pods()], playing())
}

/// Same, with the snapshot adjusted before start.
pub fn world_where(answer: Option<RouteResponse>, f: impl FnOnce(&mut DeviceSnapshot)) -> Harness {
    let mut snap = playing();
    f(&mut snap);
    world_with(reclaim_config(&["AirPods Max"]), answer, vec![connected_air_pods()], snap)
}

impl Harness {
    pub fn set_answer(&self, a: Option<RouteResponse>) {
        *self.routing.answer.borrow_mut() = a;
    }
    pub fn user(&self, idle: Option<f64>) {
        self.set(|s| s.presence = PresenceFacts { input_idle: idle, ..Default::default() });
    }
}
