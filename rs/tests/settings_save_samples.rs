//! The settings-save failures reported against the window, one test each, replayed through the
//! real save path (draft -> managed -> merge -> file -> reopen). Only `Session` touches the write
//! API, so the rest also builds against the tree before the unit model.

use cleat_rs::config::Config;
use cleat_rs::model::AudioDevice;
use cleat_rs::settings::document::{merged_from, Origin};
use cleat_rs::settings::draft::{LiveLevels, PairedDevice, SettingsDraft};
use serde_json::{json, Value};

const SAMPLE1: &str = r#"{"reclaim":["AirPods Max"],"reclaimEnabled":false,"outputVolumeHoldAgainst":["Zoom"],"outputVolumeHoldEnabled":false}"#;

fn present() -> Vec<AudioDevice> {
    vec![
        AudioDevice::new(1, "Wireless microphone", "uid-1", true, false),
        AudioDevice::new(2, "Brio 100", "uid-2", true, false),
        AudioDevice::new(3, "AirPods Max", "uid-3", true, true),
        AudioDevice::new(4, "MacBook Pro Speakers", "uid-4", false, true),
    ]
}

fn air_pods() -> Vec<PairedDevice> {
    vec![PairedDevice { name: "AirPods Max".into(), address: "AA:01".into(), is_connected: true, minor_type: Some("Headphones".into()) }]
}

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

struct Session {
    draft: SettingsDraft,
    origin: Origin,
    file: String,
}

impl Session {
    fn open(raw: &str, present: &[AudioDevice], paired: &[PairedDevice]) -> Self {
        let config: Config = serde_json::from_str(raw).unwrap();
        let draft = SettingsDraft::make(&config, present, paired, &LiveLevels::default());
        let origin = Origin::new(Some(raw.as_bytes()), draft.managed());
        Self { draft, origin, file: raw.into() }
    }

    fn save(&mut self) -> Value {
        let out = merged_from(Some(self.file.as_bytes()), &self.draft.managed(), Some(&self.origin)).unwrap();
        self.file = String::from_utf8(out).unwrap();
        serde_json::from_str(&self.file).unwrap()
    }

    fn reopen(&self, present: &[AudioDevice], paired: &[PairedDevice]) -> SettingsDraft {
        SettingsDraft::make(&serde_json::from_str(&self.file).unwrap(), present, paired, &LiveLevels::default())
    }

    fn config(&self) -> Config {
        serde_json::from_str(&self.file).unwrap()
    }
}

#[test]
fn sample1_turning_a_parked_feature_on_reaches_the_daemon() {
    let mut x = Session::open(SAMPLE1, &present(), &air_pods());
    x.draft.set_reclaim_enabled(true);
    x.draft.set_hold_enabled(true);
    x.save();
    assert_eq!(x.config().reclaim_active(), ["AirPods Max"], "{}", x.file);
    assert_eq!(x.config().hold_against_active(), ["Zoom"], "{}", x.file);
}

#[test]
fn sample2_a_parked_list_survives_an_unrelated_save() {
    let mut x = Session::open(SAMPLE1, &present(), &air_pods());
    x.draft.headphones_take_over = !x.draft.headphones_take_over;
    let out = x.save();
    for key in ["reclaim", "reclaimEnabled", "outputVolumeHoldAgainst", "outputVolumeHoldEnabled"] {
        assert_eq!(out[key], serde_json::from_str::<Value>(SAMPLE1).unwrap()[key], "{key}: {out}");
    }
}

#[test]
fn sample3_a_device_is_never_both_preferred_and_not_used() {
    let raw = r#"{"input":["Wireless microphone","Brio 100","AirPods Max"],"blockedInput":["AirPods Max"]}"#;
    let mut x = Session::open(raw, &present(), &[]);
    assert_eq!(x.draft.input.priority(), s(&["Wireless microphone", "Brio 100"]));
    x.draft.input.move_listed(1, 0);
    let out = x.save();
    assert_eq!(out["input"], json!(["Brio 100", "Wireless microphone"]), "{out}");
    assert_eq!(out["blockedInput"], json!(["AirPods Max"]), "{out}");
}

#[test]
fn sample3_not_used_takes_a_device_out_of_the_order() {
    let mut x = Session::open(r#"{"input":["Wireless microphone","Brio 100"]}"#, &present(), &[]);
    x.draft.input.set_blocked("Brio 100", true);
    assert_eq!(x.draft.input.priority(), s(&["Wireless microphone"]));
    let out = x.save();
    assert_eq!(out["input"], json!(["Wireless microphone"]), "{out}");
    assert_eq!(out["blockedInput"], json!(["Brio 100"]), "{out}");
}

#[test]
fn i1_on_then_off_leaves_the_file_as_it_was() {
    let mut x = Session::open(SAMPLE1, &present(), &air_pods());
    x.draft.set_reclaim_enabled(true);
    x.draft.set_hold_enabled(true);
    x.draft.set_reclaim_enabled(false);
    x.draft.set_hold_enabled(false);
    assert_eq!(x.save(), serde_json::from_str::<Value>(SAMPLE1).unwrap());
}

#[test]
fn n1_on_save_off_save_keeps_the_lists_switched_off() {
    let mut x = Session::open(SAMPLE1, &present(), &air_pods());
    x.draft.set_reclaim_enabled(true);
    x.draft.set_hold_enabled(true);
    x.save();
    x.draft.set_reclaim_enabled(false);
    x.draft.set_hold_enabled(false);
    let out = x.save();
    assert_eq!((&out["reclaim"], &out["reclaimEnabled"]), (&json!(["AirPods Max"]), &json!(false)), "{out}");
    assert_eq!((&out["outputVolumeHoldAgainst"], &out["outputVolumeHoldEnabled"]), (&json!(["Zoom"]), &json!(false)), "{out}");
    let again = x.reopen(&present(), &air_pods());
    assert!(!again.reclaim_enabled && !again.hold_enabled);
    assert!(x.config().reclaim_active().is_empty() && x.config().hold_against_active().is_empty());
}

#[test]
fn n2_adding_a_not_used_device_to_the_order_lifts_not_used() {
    let mut x = Session::open(r#"{"input":["Wireless microphone"],"blockedInput":["Brio 100"]}"#, &present(), &[]);
    x.draft.input.set_listed("Brio 100", true);
    let out = x.save();
    assert_eq!(out["input"], json!(["Wireless microphone", "Brio 100"]), "{out}");
    assert_eq!(out["blockedInput"], json!([]), "{out}");
}

#[test]
fn n4_an_untouched_unit_keeps_its_values() {
    let raw = r#"{"input":["Brio 100","Brio 100"],"blockedInput":["Z mic","Brio 100"],"inputVolume":{"Brio 100":33.5,"*":72.4},"balance":0.455,"reclaim":["AirPods Max","AirPods Max"]}"#;
    let mut x = Session::open(raw, &present(), &air_pods());
    x.draft.headphones_take_over = !x.draft.headphones_take_over;
    let out = x.save();
    let before: Value = serde_json::from_str(raw).unwrap();
    for (key, value) in before.as_object().unwrap() {
        assert_eq!(&out[key], value, "{key}: {out}");
    }
    assert!(out.get("output").is_none() && out.get("blockedOutput").is_none(), "{out}");
}

/// B-1287: unticking the last headset leaves the switch on, through a save and a reopen; the
/// daemon then has nothing to ask for.
#[test]
fn n5_unticking_the_last_headset_keeps_reclaim_on() {
    let mut x = Session::open(r#"{"reclaim":["AirPods Max"]}"#, &present(), &air_pods());
    x.draft.set_headset("AirPods Max", false);
    assert!(x.draft.reclaim_enabled);
    let out = x.save();
    assert_eq!((&out["reclaim"], &out["reclaimEnabled"]), (&json!([]), &json!(true)), "{out}");
    assert!(x.reopen(&present(), &air_pods()).reclaim_enabled, "{}", x.file);
    assert!(x.config().reclaim_active().is_empty());
}

/// B-1287: with no headset at all the switch still goes on and stays on after a reopen.
#[test]
fn n5_reclaim_goes_on_without_a_headset() {
    let mut x = Session::open("{}", &[], &[]);
    x.draft.set_reclaim_enabled(true);
    assert!(x.draft.reclaim_enabled);
    x.save();
    assert!(x.reopen(&[], &[]).reclaim_enabled, "{}", x.file);
    x.draft.set_reclaim_enabled(false);
    let out = x.save();
    assert!(out.get("reclaimEnabled").is_none(), "off with nothing ticked is Swift's []: {out}");
    assert!(!x.reopen(&[], &[]).reclaim_enabled, "{}", x.file);
}
