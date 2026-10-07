use cleat_rs::config::Config;
use cleat_rs::model::AudioDevice;
use cleat_rs::settings::draft::{DeviceList, LiveLevels, PairedDevice, SettingsDraft, DEFAULT_HOLD_AGAINST};

fn out(id: u32, name: &str) -> AudioDevice {
    AudioDevice::new(id, name, &format!("uid-{id}"), false, true)
}

fn inp(id: u32, name: &str) -> AudioDevice {
    AudioDevice::new(id, name, &format!("uid-{id}"), true, false)
}

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

fn entries(list: &DeviceList) -> Vec<String> {
    list.rows.iter().map(|r| r.entry.clone()).collect()
}

#[test]
fn rows_are_priority_then_blocked_then_present() {
    let present = [out(1, "Zed"), out(2, "Alpha"), out(3, "B")];
    let list = DeviceList::make(&s(&["B", "Gone", "B"]), &s(&["X"]), &present, false);
    assert_eq!(entries(&list), s(&["B", "Gone", "X", "Alpha", "Zed"]));
    assert!(!list.rows[1].is_connected);
    assert!(list.rows[2].is_blocked && !list.rows[2].is_listed);
}

#[test]
fn listing_lands_on_the_boundary() {
    let present = [out(1, "A"), out(2, "B"), out(3, "C")];
    let mut list = DeviceList::make(&s(&["A"]), &[], &present, false);
    list.set_listed("C", true);
    assert_eq!(list.priority(), s(&["A", "C"]));
    assert_eq!(entries(&list), s(&["A", "C", "B"]));
    list.set_listed("A", false);
    assert_eq!(entries(&list), s(&["C", "A", "B"]));
}

#[test]
fn moving_up_and_down() {
    let mut list = DeviceList::make(&s(&["A", "B", "C"]), &[], &[], false);
    list.move_listed(2, 1);
    assert_eq!(list.priority(), s(&["A", "C", "B"]));
    list.move_listed(0, 2);
    assert_eq!(list.priority(), s(&["C", "A", "B"]));
}

fn paired(name: &str, connected: bool, kind: Option<&str>) -> PairedDevice {
    PairedDevice { name: name.into(), address: format!("AA:{name}"), is_connected: connected, minor_type: kind.map(String::from) }
}

#[test]
fn headset_options_come_in_source_order() {
    let p = [paired("Zeta", false, Some("Headphones")), paired("Beta", true, Some("Headphones")), paired("Mouse", true, Some("Mouse")), paired("Box", false, None)];
    let opts = SettingsDraft::make_headset_options(&s(&["Mine"]), &p, &[]);
    let names: Vec<_> = opts.iter().map(|o| (o.entry.as_str(), o.is_other)).collect();
    assert_eq!(names, [("Mine", false), ("Beta", false), ("Zeta", false), ("Box", true)]);
}

#[test]
fn turning_reclaim_on_ticks_the_first_connected_headset() {
    let p = [paired("A", false, Some("Headphones")), paired("B", true, Some("Headphones"))];
    let mut d = SettingsDraft::make(&Config::disabled(), &[], &p, &LiveLevels::default());
    d.set_reclaim_enabled(true);
    assert_eq!(d.managed().reclaim, s(&["B"]));
    d.set_reclaim_enabled(false);
    assert!(d.managed().reclaim.is_empty());
}

#[test]
fn managed_rounds_volumes_and_balance() {
    let mut d = SettingsDraft::make(&Config::disabled(), &[], &[], &LiveLevels::default());
    d.wildcard_enabled = true;
    d.wildcard_percent = 62.6;
    d.balance_enabled = true;
    d.balance = 0.333;
    let m = d.managed();
    assert_eq!(m.input_volume.get("*"), Some(&63.0));
    assert_eq!(m.balance, Some(0.33));
}

#[test]
fn hold_starts_from_the_default_and_ends_with_the_last_app() {
    let mut d = SettingsDraft::make(&Config::disabled(), &[], &[], &LiveLevels::default());
    assert!(!d.hold_enabled);
    d.set_hold_enabled(true);
    assert_eq!(d.managed().hold_against, Some(s(&[DEFAULT_HOLD_AGAINST])));
    d.remove_hold_app(DEFAULT_HOLD_AGAINST);
    assert!(!d.hold_enabled);
    assert_eq!(d.managed().hold_against, None);
}

#[test]
fn volume_candidates_leave_out_set_devices() {
    let present = [inp(1, "Mic"), inp(2, "Brio"), out(3, "Spk")];
    let mut d = SettingsDraft::make(&Config::disabled(), &present, &[], &LiveLevels::default());
    d.add_volume("Mic", &present);
    assert_eq!(d.volume_candidates(&present), s(&["Brio"]));
    assert_eq!(d.named_volumes[0].percent, 100.0);
}

#[test]
fn unset_values_start_from_the_live_reading() {
    let live = LiveLevels { input_volume: Some(62.4), balance: Some(0.333), ..Default::default() };
    let d = SettingsDraft::make(&Config::disabled(), &[], &[], &live);
    assert_eq!((d.wildcard_percent, d.balance), (62.0, 0.33));
    let d = SettingsDraft::make(&Config::disabled(), &[], &[], &LiveLevels::default());
    assert_eq!((d.wildcard_percent, d.balance), (100.0, 0.5));
}

#[test]
fn a_rust_only_switch_left_off_reads_as_off() {
    let config = Config { reclaim: s(&["A"]), reclaim_enabled: false, ..Config::disabled() };
    let d = SettingsDraft::make(&config, &[], &[], &LiveLevels::default());
    assert!(!d.reclaim_enabled);
}
