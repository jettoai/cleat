//! The four save invariants over every file state x edit sequence x save timing (no sampling):
//! I1 an untouched save keeps every value, I2 a saved file reopens as shown, I3 the file keeps
//! what the window promises (switches, stances, and no pin rule ever picks a "not used" device),
//! I4 an edit changes only its own unit. States and sequences are enumerated as in the plan §2.

use std::collections::HashSet;
use std::sync::OnceLock;

use cleat_rs::config::Config;
use cleat_rs::model::{Action, AudioDevice, DeviceSnapshot, Liveness, TRANSPORT_BLUETOOTH};
use cleat_rs::rules::{headphones_takeover, input_pin, output_pin};
use cleat_rs::settings::document::{merged_from, Managed, Origin, Unit, UNITS};
use cleat_rs::settings::draft::{DeviceList, LiveLevels, PairedDevice, SettingsDraft, Stance};
use serde_json::{json, Value};

fn present() -> Vec<AudioDevice> {
    vec![
        AudioDevice::new(1, "Wireless microphone", "uid-1", true, false),
        AudioDevice::new(2, "Brio 100", "uid-2", true, false),
        // Bluetooth, so headphone takeover has something to act on in the rule check.
        AudioDevice::with_transport(3, "AirPods Max", "uid-3", true, true, TRANSPORT_BLUETOOTH),
        AudioDevice::new(4, "MacBook Pro Speakers", "uid-4", false, true),
    ]
}

fn paired() -> Vec<PairedDevice> {
    let one = |name: &str, address: &str, on: bool| PairedDevice {
        name: name.into(),
        address: address.into(),
        is_connected: on,
        minor_type: Some("Headphones".into()),
    };
    vec![one("AirPods Max", "AA:01", true), one("Bose QC", "AA:02", false)]
}

type Variant = Vec<(&'static str, Value)>;

fn product(a: &[Option<Value>], ka: &'static str, b: &[Option<Value>], kb: &'static str) -> Vec<Variant> {
    let mut out = vec![];
    for x in a {
        for y in b {
            let mut v = vec![];
            if let Some(x) = x {
                v.push((ka, x.clone()));
            }
            if let Some(y) = y {
                v.push((kb, y.clone()));
            }
            out.push(v);
        }
    }
    out
}

fn switch() -> Vec<Option<Value>> {
    vec![None, Some(json!(true)), Some(json!(false))]
}

fn reclaim_variants() -> Vec<Variant> {
    let lists = [None, Some(json!([])), Some(json!(["AirPods Max"])), Some(json!(["AirPods Max", "AirPods Max"]))];
    product(&lists, "reclaim", &switch(), "reclaimEnabled")
}

fn hold_variants() -> Vec<Variant> {
    let lists =
        [None, Some(json!([])), Some(json!(["Zoom"])), Some(json!(["Zoom", "Zoom"])), Some(json!(["Parallels Desktop"]))];
    product(&lists, "outputVolumeHoldAgainst", &switch(), "outputVolumeHoldEnabled")
}

fn single_unit_variants() -> Vec<(Unit, Vec<Variant>)> {
    let input = vec![
        vec![],
        vec![("input", json!([])), ("blockedInput", json!([]))],
        vec![("input", json!(["Brio 100"]))],
        vec![("blockedInput", json!(["Brio 100"]))],
        vec![("input", json!(["Brio 100"])), ("blockedInput", json!(["Brio 100"]))],
        vec![("input", json!(["Brio 100"])), ("blockedInput", json!(["uid-2"]))],
        vec![("input", json!(["Wireless microphone", "Brio 100", "AirPods Max"])), ("blockedInput", json!(["AirPods Max"]))],
    ];
    let output = vec![
        vec![],
        vec![("output", json!(["MacBook Pro Speakers"]))],
        vec![("blockedOutput", json!(["AirPods Max"]))],
        vec![("output", json!(["AirPods Max"])), ("blockedOutput", json!(["AirPods Max"]))],
        vec![("output", json!(["MacBook Pro Speakers", "MacBook Pro Speakers"]))],
    ];
    let takeover = vec![vec![], vec![("headphonesTakeOver", json!(true))], vec![("headphonesTakeOver", json!(false))]];
    let volume = [json!({}), json!({"Brio 100": 33}), json!({"Brio 100": 33.5}), json!({"*": 72.4}), json!({"*": 72, "Brio 100": 33.5})];
    let volume = std::iter::once(vec![]).chain(volume.into_iter().map(|v| vec![("inputVolume", v)])).collect();
    let balance = vec![vec![], vec![("balance", json!(0.45))], vec![("balance", json!(0.455))]];
    vec![
        (Unit::Reclaim, reclaim_variants()),
        (Unit::Hold, hold_variants()),
        (Unit::InputStance, input),
        (Unit::OutputStance, output),
        (Unit::Takeover, takeover),
        (Unit::InputVolume, volume),
        (Unit::Balance, balance),
    ]
}

/// One-unit variants with the rest missing, plus reclaim x hold in full; each with the units it varies.
fn states() -> Vec<(Value, Vec<Unit>)> {
    let fixed = json!({"liveness": {"Wireless microphone": {"zeroSeconds": 5}}, "launchAtLogin": false, "zeta": 1});
    let build = |parts: &[&Variant]| {
        let mut o = fixed.as_object().unwrap().clone();
        for part in parts {
            for (k, v) in part.iter() {
                o.insert(k.to_string(), v.clone());
            }
        }
        Value::Object(o)
    };
    let mut out = vec![];
    for (unit, variants) in single_unit_variants() {
        for v in &variants {
            out.push((build(&[v]), vec![unit]));
        }
    }
    for r in &reclaim_variants() {
        for h in &hold_variants() {
            out.push((build(&[r, h]), vec![Unit::Reclaim, Unit::Hold]));
        }
    }
    out
}

type Edit = fn(&mut SettingsDraft);

fn first_hold(d: &mut SettingsDraft) {
    if let Some(n) = d.hold_against.first().cloned() {
        d.remove_hold_app(&n);
    }
}

fn first_volume(d: &mut SettingsDraft) {
    if let Some(e) = d.named_volumes.first().map(|v| v.entry.clone()) {
        d.remove_volume(&e);
    }
}

/// The 26 edits the window can make (plan §2.2), each with the unit it belongs to.
fn edits() -> Vec<(&'static str, Unit, Edit)> {
    vec![
        ("in.move", Unit::InputStance, |d| d.input.move_listed(1, 0)),
        ("in.list+", Unit::InputStance, |d| d.input.set_listed("Brio 100", true)),
        ("in.list-", Unit::InputStance, |d| d.input.set_listed("Brio 100", false)),
        ("in.block+", Unit::InputStance, |d| d.input.set_blocked("Brio 100", true)),
        ("in.block-", Unit::InputStance, |d| d.input.set_blocked("Brio 100", false)),
        ("out.move", Unit::OutputStance, |d| d.output.move_listed(1, 0)),
        ("out.list+", Unit::OutputStance, |d| d.output.set_listed("AirPods Max", true)),
        ("out.list-", Unit::OutputStance, |d| d.output.set_listed("AirPods Max", false)),
        ("out.block+", Unit::OutputStance, |d| d.output.set_blocked("AirPods Max", true)),
        ("out.block-", Unit::OutputStance, |d| d.output.set_blocked("AirPods Max", false)),
        ("hold.on", Unit::Hold, |d| d.set_hold_enabled(true)),
        ("hold.off", Unit::Hold, |d| d.set_hold_enabled(false)),
        ("hold.add", Unit::Hold, |d| d.add_hold_app("Zoom")),
        ("hold.remove", Unit::Hold, first_hold),
        ("reclaim.on", Unit::Reclaim, |d| d.set_reclaim_enabled(true)),
        ("reclaim.off", Unit::Reclaim, |d| d.set_reclaim_enabled(false)),
        ("tick.airpods", Unit::Reclaim, |d| d.set_headset("AirPods Max", true)),
        ("untick.airpods", Unit::Reclaim, |d| d.set_headset("AirPods Max", false)),
        ("tick.bose", Unit::Reclaim, |d| d.set_headset("Bose QC", true)),
        ("vol.add", Unit::InputVolume, |d| d.add_volume("Brio 100", &present())),
        ("vol.remove", Unit::InputVolume, first_volume),
        ("vol.wildcard", Unit::InputVolume, |d| d.wildcard_enabled = !d.wildcard_enabled),
        ("vol.wildcard40", Unit::InputVolume, |d| d.wildcard_percent = 40.0),
        ("vol.named40", Unit::InputVolume, |d| {
            if let Some(v) = d.named_volumes.first_mut() {
                v.percent = 40.0
            }
        }),
        ("balance", Unit::Balance, |d| d.balance_enabled = !d.balance_enabled),
        ("takeover", Unit::Takeover, |d| d.headphones_take_over = !d.headphones_take_over),
    ]
}

/// Empty; each edit alone; every ordered pair within a unit; each edit then the takeover switch.
/// Pairs within a unit run only on the states that vary that unit (plan §2.2, to stay within the
/// time budget): the other units are missing there, so pairing their edits adds nothing.
fn sequences(varied: &[Unit]) -> Vec<Vec<usize>> {
    let all = edits();
    let takeover = all.len() - 1;
    let mut out = vec![vec![]];
    out.extend((0..all.len()).map(|i| vec![i]));
    for i in 0..all.len() {
        for j in 0..all.len() {
            if all[i].1 == all[j].1 && varied.contains(&all[i].1) {
                out.push(vec![i, j]);
            }
        }
    }
    out.extend((0..all.len()).map(|i| vec![i, takeover]));
    out
}

fn sorted(m: &Managed) -> Managed {
    let mut m = m.clone();
    m.blocked_input.sort();
    m.blocked_output.sort();
    m
}

fn dedup(list: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    list.iter().filter(|x| seen.insert(*x)).cloned().collect()
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array().map_or(vec![], |a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
}

fn check_stance(list: &DeviceList, priority: &[String], blocked: &[String], written: bool, side: &str) -> Vec<String> {
    let mut out = vec![];
    for r in &list.rows {
        match r.stance {
            Stance::Listed if !priority.contains(&r.entry) || blocked.contains(&r.entry) => {
                out.push(format!("{side} listed row {} not in order alone", r.entry))
            }
            Stance::Blocked if !blocked.contains(&r.entry) => out.push(format!("{side} blocked row {} not written", r.entry)),
            _ => {}
        }
    }
    if written && priority.iter().any(|e| blocked.contains(e)) {
        out.push(format!("{side} both preferred and not used: {priority:?} / {blocked:?}"));
    }
    out
}

/// Every action a pin rule takes on this config, with every device present and live and each one
/// in turn the current default, must target a device outside the blocked list of its side.
fn rule_violations(config: &Config) -> Vec<String> {
    let devices = present();
    let mut out = vec![];
    for current in &devices {
        let snapshot = DeviceSnapshot {
            devices: devices.clone(),
            default_input: current.has_input.then_some(current.id),
            default_output: current.has_output.then_some(current.id),
            liveness: devices.iter().map(|d| (d.uid.clone(), Liveness::Live)).collect(),
            arrived: devices.iter().map(|d| d.uid.clone()).collect(),
            ..Default::default()
        };
        let actions = [
            input_pin::reconcile(&snapshot, config),
            output_pin::reconcile(&snapshot, config),
            headphones_takeover::reconcile(&snapshot, config),
        ];
        for action in actions.iter().flatten() {
            let (id, blocked) = match action {
                Action::SetDefaultInput(id, _) => (*id, &config.blocked_input),
                Action::SetDefaultOutput(id, _) => (*id, &config.blocked_output),
                _ => continue,
            };
            if devices.iter().any(|d| d.id == id && d.is_listed(blocked)) {
                out.push(format!("rule picked a not-used device: {}", action.reason()));
            }
        }
    }
    out
}

#[derive(Default)]
struct Report {
    runs: usize,
    i1: Vec<String>,
    i2: Vec<String>,
    i3: Vec<String>,
    i4: Vec<String>,
}

fn run() -> &'static Report {
    static REPORT: OnceLock<Report> = OnceLock::new();
    REPORT.get_or_init(|| {
        let (present, paired, all) = (present(), paired(), edits());
        let mut report = Report::default();
        let mut ruled = HashSet::new();
        let owned: HashSet<&str> = UNITS.iter().flat_map(|(_, keys)| keys.iter().copied()).collect();
        for (state, varied) in states() {
            let raw = state.to_string();
            let config: Config = serde_json::from_str(&raw).unwrap();
            for seq in sequences(&varied) {
                for save_each in [false, true] {
                    report.runs += 1;
                    let mut draft = SettingsDraft::make(&config, &present, &paired, &LiveLevels::default());
                    let at_load = draft.managed();
                    let origin = Origin::new(Some(raw.as_bytes()), at_load.clone());
                    let mut file = raw.clone();
                    let mut save = |d: &SettingsDraft| {
                        file = String::from_utf8(merged_from(Some(file.as_bytes()), &d.managed(), Some(&origin)).unwrap()).unwrap();
                    };
                    for &i in &seq {
                        (all[i].2)(&mut draft);
                        if save_each {
                            save(&draft);
                        }
                    }
                    if !save_each || seq.is_empty() {
                        save(&draft);
                    }
                    let name: Vec<&str> = seq.iter().map(|&i| all[i].0).collect();
                    let tag = format!("{raw} {name:?} each={save_each}");
                    let out: Value = serde_json::from_str(&file).unwrap();
                    let saved: Config = serde_json::from_value(out.clone()).unwrap();
                    let m = draft.managed();

                    if seq.is_empty() && out != state {
                        report.i1.push(format!("{tag}: {out}"));
                    }

                    let back = SettingsDraft::make(&saved, &present, &paired, &LiveLevels::default()).managed();
                    if sorted(&back) != sorted(&m) {
                        report.i2.push(format!("{tag}: shown {m:?} reopened {back:?}"));
                    }

                    let mut i3 = vec![];
                    let reclaim = if m.reclaim_on { m.reclaim.clone() } else { vec![] };
                    if dedup(saved.reclaim_active()) != reclaim {
                        i3.push(format!("reclaim active {:?} vs shown {reclaim:?}", saved.reclaim_active()));
                    }
                    let hold = if m.hold_on { m.hold_against.clone() } else { vec![] };
                    if saved.hold_against_active() != hold {
                        i3.push(format!("hold active {:?} vs shown {hold:?}", saved.hold_against_active()));
                    }
                    // Written from the draft (not kept as loaded) when it no longer encodes as at load.
                    let written = |u: Unit| at_load.unit(u) != m.unit(u);
                    i3.extend(check_stance(&draft.input, &strings(&out["input"]), &strings(&out["blockedInput"]), written(Unit::InputStance), "input"));
                    i3.extend(check_stance(&draft.output, &strings(&out["output"]), &strings(&out["blockedOutput"]), written(Unit::OutputStance), "output"));
                    if ruled.insert(file.clone()) {
                        i3.extend(rule_violations(&saved));
                    }
                    report.i3.extend(i3.into_iter().map(|e| format!("{tag}: {e}")));

                    let touched: Vec<Unit> = seq.iter().map(|&i| all[i].1).collect();
                    let before = state.as_object().unwrap();
                    let after = out.as_object().unwrap();
                    for (unit, keys) in UNITS {
                        if touched.contains(&unit) {
                            continue;
                        }
                        for key in keys {
                            if before.get(*key) != after.get(*key) {
                                report.i4.push(format!("{tag}: {key} {:?} -> {:?}", before.get(*key), after.get(*key)));
                            }
                        }
                    }
                    for key in before.keys().chain(after.keys()).filter(|k| !owned.contains(k.as_str())) {
                        if before.get(key) != after.get(key) {
                            report.i4.push(format!("{tag}: unowned {key} changed"));
                        }
                    }
                }
            }
        }
        report
    })
}

fn verdict(name: &str, fails: &[String]) {
    let report = run();
    assert!(report.runs > 30_000, "only {} runs", report.runs);
    let first: Vec<&String> = fails.iter().take(20).collect();
    assert!(fails.is_empty(), "{name}: {} violations in {} runs:\n{first:#?}", fails.len(), report.runs);
}

#[test]
fn untouched_saves_keep_every_value() {
    verdict("I1", &run().i1);
}

#[test]
fn saved_files_reopen_as_shown() {
    verdict("I2", &run().i2);
}

#[test]
fn saved_files_keep_the_window_promises() {
    verdict("I3", &run().i3);
}

#[test]
fn edits_change_only_their_unit() {
    verdict("I4", &run().i4);
}
