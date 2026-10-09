use cleat_rs::config::Config;
use cleat_rs::model::AudioDevice;
use cleat_rs::settings::draft::{DeviceList, HeadsetBox, LiveLevels, PairedDevice, ReclaimHint, SettingsDraft, Stance, DEFAULT_HOLD_AGAINST};

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
    assert_eq!(list.rows[2].stance, Stance::Blocked);
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
    let m = d.managed();
    assert!(!m.reclaim_on);
    assert_eq!(m.reclaim, s(&["B"]));
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
    let m = d.managed();
    assert_eq!((m.hold_on, m.hold_against), (true, s(&[DEFAULT_HOLD_AGAINST])));
    d.remove_hold_app(DEFAULT_HOLD_AGAINST);
    assert!(!d.hold_enabled);
    let m = d.managed();
    assert_eq!((m.hold_on, m.hold_against), (false, vec![]));
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
    let config = Config { reclaim: s(&["A"]), reclaim_enabled: Some(false), ..Config::disabled() };
    let d = SettingsDraft::make(&config, &[], &[], &LiveLevels::default());
    assert!(!d.reclaim_enabled);
}

#[test]
fn entries_naming_one_device_are_one_row_the_way_the_daemon_matches() {
    let present = [AudioDevice::new(3, "AirPods Max", "uid-3", true, true)];
    for blocked in ["AirPods\u{00A0}Max", "uid-3"] {
        let list = DeviceList::make(&s(&["AirPods Max"]), &s(&[blocked]), &present, true);
        assert_eq!(entries(&list), s(&["AirPods Max"]), "{blocked}");
        assert_eq!(list.rows[0].stance, Stance::Blocked, "{blocked}");
        assert!(list.priority().is_empty(), "{blocked}");
    }
}

#[test]
fn a_headset_whose_output_is_not_used_reads_as_blocked() {
    let config = Config {
        reclaim: s(&["AirPods Max"]),
        reclaim_enabled: Some(true),
        blocked_output: s(&["AirPods\u{00A0}Max"]),
        ..Config::disabled()
    };
    let p = [paired("AirPods Max", true, Some("Headphones")), paired("Bose", true, Some("Headphones"))];
    let present = [out(3, "AirPods Max")];
    let mut d = SettingsDraft::make(&config, &present, &p, &LiveLevels::default());
    let blocked: Vec<_> = d.headsets.iter().map(|h| (h.entry.clone(), d.headset_blocked(h))).collect();
    assert_eq!(blocked, [("AirPods Max".to_string(), true), ("Bose".to_string(), false)]);
    d.output.set_blocked("AirPods\u{00A0}Max", false);
    assert!(!d.headset_blocked(&d.headsets[0].clone()));
}

/// B-1287: a headset whose output is "not used" never shows a tick, can be unticked but not
/// ticked, and when every ticked headset is blocked the page says nothing will happen.
#[test]
fn blocked_headsets_unblocked_partly_blocked_and_all_blocked() {
    let p = [paired("AirPods Max", true, Some("Headphones")), paired("Bose", true, Some("Headphones"))];
    let make = |reclaim: &[&str], blocked: &[&str]| {
        let config = Config { reclaim: s(reclaim), reclaim_enabled: Some(true), blocked_output: s(blocked), ..Config::disabled() };
        SettingsDraft::make(&config, &[out(3, "AirPods Max")], &p, &LiveLevels::default())
    };
    let boxes = |d: &SettingsDraft| d.headsets.iter().map(|h| d.headset_box(h)).collect::<Vec<_>>();

    let d = make(&["AirPods Max", "Bose"], &[]);
    assert_eq!(boxes(&d), [HeadsetBox::Plain(true), HeadsetBox::Plain(true)]);
    assert!(!d.all_ticked_blocked());

    let mut d = make(&["AirPods Max", "Bose"], &["AirPods Max"]);
    assert_eq!(boxes(&d), [HeadsetBox::BlockedTicked, HeadsetBox::Plain(true)]);
    assert!(!d.all_ticked_blocked());
    d.set_headset("AirPods Max", false);
    assert_eq!(boxes(&d), [HeadsetBox::BlockedOff, HeadsetBox::Plain(true)]);
    d.set_headset("AirPods Max", true);
    assert_eq!(boxes(&d), [HeadsetBox::BlockedOff, HeadsetBox::Plain(true)], "a blocked headset cannot be ticked");

    let mut d = make(&["AirPods Max"], &["AirPods Max"]);
    assert_eq!(boxes(&d), [HeadsetBox::BlockedTicked, HeadsetBox::Plain(false)]);
    assert!(d.all_ticked_blocked());
    d.set_headset("Bose", true);
    assert!(!d.all_ticked_blocked());
}

/// B-1287: turning reclaim on ticks the first connected headset that is not "not used".
#[test]
fn turning_reclaim_on_skips_a_blocked_headset() {
    let p = [paired("AirPods Max", true, Some("Headphones")), paired("Bose", true, Some("Headphones"))];
    let make = |blocked: &[&str]| {
        let config = Config { blocked_output: s(blocked), ..Config::disabled() };
        SettingsDraft::make(&config, &[out(3, "AirPods Max")], &p, &LiveLevels::default())
    };
    let ticked = |d: &SettingsDraft| d.headsets.iter().filter(|h| h.is_selected).map(|h| h.entry.clone()).collect::<Vec<_>>();

    let mut d = make(&["AirPods Max"]);
    d.set_reclaim_enabled(true);
    assert_eq!(ticked(&d), ["Bose"]);

    let mut d = make(&[]);
    d.set_reclaim_enabled(true);
    assert_eq!(ticked(&d), ["AirPods Max"]);

    let mut d = make(&["AirPods Max", "Bose"]);
    d.set_reclaim_enabled(true);
    assert!(ticked(&d).is_empty(), "every headset blocked: nothing to tick");
    assert!(d.reclaim_enabled, "the switch stays on and the page says why");
    assert_eq!(d.reclaim_hint(), Some(ReclaimHint::AllHeadsetsBlocked));
}

/// B-1287: with reclaim on but nothing it can act on, the page names which of four reasons; off
/// or acting, it says nothing. A classless device (a phone) is never ticked on Albert's behalf.
#[test]
fn reclaim_hint_names_why_nothing_will_happen() {
    let p = [paired("AirPods Max", true, Some("Headphones")), paired("Bose", true, Some("Headphones")), paired("Phone", true, None)];
    let make = |reclaim: &[&str], blocked: &[&str], paired: &[PairedDevice]| {
        let config = Config { reclaim: s(reclaim), blocked_output: s(blocked), ..Config::disabled() };
        SettingsDraft::make(&config, &[out(3, "AirPods Max")], paired, &LiveLevels::default())
    };

    let mut d = make(&[], &[], &[]);
    assert_eq!(d.reclaim_hint(), None, "off says nothing");
    d.set_reclaim_enabled(true);
    assert_eq!((d.reclaim_enabled, d.reclaim_hint()), (true, Some(ReclaimHint::NoHeadsets)));

    let mut d = make(&[], &["AirPods Max", "Bose"], &p);
    d.set_reclaim_enabled(true);
    assert_eq!(d.reclaim_hint(), Some(ReclaimHint::AllHeadsetsBlocked));
    assert!(!d.headsets.iter().any(|h| h.is_selected), "the phone is not ticked for him");

    let mut d = make(&["AirPods Max"], &[], &p);
    assert_eq!(d.reclaim_hint(), None, "a ticked usable headset acts");
    d.set_headset("AirPods Max", false);
    assert_eq!((d.reclaim_enabled, d.reclaim_hint()), (true, Some(ReclaimHint::NoneTicked)));

    let d = make(&["AirPods Max"], &["AirPods Max"], &p);
    assert_eq!(d.reclaim_hint(), Some(ReclaimHint::AllTickedBlocked));

    let mut d = make(&["AirPods Max"], &["AirPods Max"], &p);
    d.set_reclaim_enabled(false);
    assert_eq!(d.reclaim_hint(), None, "off says nothing");
}

#[test]
fn launch_at_login_comes_from_the_config_and_goes_back_out() {
    let on = SettingsDraft::make(&Config::disabled(), &[], &[], &LiveLevels::default());
    assert!(on.launch_at_login && on.managed().launch_at_login);
    let off = Config { launch_at_login: false, ..Config::default() };
    let d = SettingsDraft::make(&off, &[], &[], &LiveLevels::default());
    assert!(!d.launch_at_login && !d.managed().launch_at_login);
}
