//! B-2987: the settings window writes the dragged order to config.json; the daemon's hot reload
//! must move the default device to the new first choice. The "after" files are the bytes the real
//! settings window wrote (extra `blockedInput`/`blockedOutput` keys, compact spacing), unedited.

mod common;

use cleat_rs::engine::Event;
use cleat_rs::model::{AudioDevice, DeviceSnapshot, TRANSPORT_USB};
use common::engine::{Harness, Opts};
use common::*;

const INPUT_BEFORE: &str = include_str!("fixtures/settings-reorder/input-before.json");
const INPUT_AFTER: &str = include_str!("fixtures/settings-reorder/input-after.json");
const OUTPUT_BEFORE: &str = include_str!("fixtures/settings-reorder/output-before.json");
const OUTPUT_AFTER: &str = include_str!("fixtures/settings-reorder/output-after.json");

fn pd400x() -> AudioDevice {
    AudioDevice::with_transport(25, "PD400X Podcast Microphone", "PD400X-UID", true, false, TRANSPORT_USB)
}

fn hot_reload(h: &mut Harness, after: &str) {
    std::fs::write(h.dir.join("config.json"), after).unwrap();
    h.engine.handle(Event::ConfigTouched { received: std::time::SystemTime::now() });
    h.drive(400);
}

#[test]
fn dragged_input_order_moves_the_default_input_after_hot_reload() {
    let snap = DeviceSnapshot {
        devices: vec![wireless(), pd400x(), brio()],
        default_input: Some(wireless().id),
        ..Default::default()
    };
    let mut h = Harness::with_json(INPUT_BEFORE, snap, Opts::default());
    assert_eq!(h.audio.snapshot.borrow().default_input, Some(wireless().id), "before: the first choice is the default");
    assert!(h.writes().is_empty(), "{:?}", h.writes());

    hot_reload(&mut h, INPUT_AFTER);

    assert_eq!(h.audio.snapshot.borrow().default_input, Some(pd400x().id));
    assert_eq!(h.writes(), vec![format!("input:{}", pd400x().id)]);
}

#[test]
fn dragged_output_order_moves_the_default_output_after_hot_reload() {
    let snap = DeviceSnapshot {
        devices: vec![wired_headphones(), mac_studio_speakers(), display_speakers()],
        default_output: Some(wired_headphones().id),
        ..Default::default()
    };
    let mut h = Harness::with_json(OUTPUT_BEFORE, snap, Opts::default());
    assert_eq!(h.audio.snapshot.borrow().default_output, Some(wired_headphones().id), "before: the first choice is the default");
    assert!(h.writes().is_empty(), "{:?}", h.writes());

    hot_reload(&mut h, OUTPUT_AFTER);

    assert_eq!(h.audio.snapshot.borrow().default_output, Some(mac_studio_speakers().id));
    assert_eq!(h.writes(), vec![format!("output:{}", mac_studio_speakers().id)]);
}
