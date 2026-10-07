//! 2026-10-07, 10:19 to 12:56, replayed through the ledger on the log's own clock (Swift
//! `OutputVolumeReplayTests.swift`, 3 tests). Parallels pulled the AirPods Max down five times;
//! Control Center, the Digital Crown and Jetto moved it 68 more. Only the five may be undone.
//! Named blind spot (Swift's too): the captures have no listener readings, so every move of
//! channel 262/263 is taken as a listener reading at that instant.

mod common;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use cleat_rs::outvol::ledger::{Ledger, JUDGE_DELAY};
use cleat_rs::outvol::writer_log::VolumeWrite;
use cleat_rs::rules::output_volume_hold::{Verdict, TOLERANCE};

enum Kind {
    Write { pid: i32, control: i64, from: f32, to: f32 },
    Crown(f32),
    Reconnect(f32),
}

struct Ev {
    at: SystemTime,
    kind: Kind,
}

struct Revert {
    at: SystemTime,
    streak_start: SystemTime,
    writer: String,
    restore: f32,
}

#[derive(Clone)]
struct Group {
    at: SystemTime,
    end: SystemTime,
    writer: String,
    to: f32,
    channels: BTreeSet<i64>,
}

fn device() -> cleat_rs::model::AudioDevice {
    common::air_pods()
}

fn writer(pid: i32) -> String {
    match pid {
        531 => "ControlCenter".into(),
        8484 => "prl_vm_app".into(),
        72504 => "Jetto".into(),
        _ => format!("pid {pid}"),
    }
}

/// `yyyy-MM-dd HH:mm:ss.SSS` at +0800.
fn stamp(date: &str, time: &str) -> SystemTime {
    let d: Vec<i64> = date.split('-').map(|x| x.parse().unwrap()).collect();
    let (hms, ms) = time.split_once('.').unwrap();
    let t: Vec<i64> = hms.split(':').map(|x| x.parse().unwrap()).collect();
    // SAFETY: zeroed tm is valid input once the fields are set.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    tm.tm_year = (d[0] - 1900) as i32;
    tm.tm_mon = (d[1] - 1) as i32;
    tm.tm_mday = d[2] as i32;
    tm.tm_hour = t[0] as i32;
    tm.tm_min = t[1] as i32;
    tm.tm_sec = t[2] as i32;
    // SAFETY: valid struct.
    let secs = unsafe { libc::timegm(&mut tm) } - 8 * 3600;
    UNIX_EPOCH + Duration::from_secs(secs as u64) + Duration::from_millis(ms.parse().unwrap())
}

fn events() -> Vec<Ev> {
    include_str!("fixtures/output_volume_replay.txt")
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
        .map(|line| {
            let f: Vec<&str> = line.split(' ').collect();
            let at = stamp(f[0], f[1]);
            let kind = match f[2] {
                "write" => Kind::Write {
                    pid: f[3].parse().unwrap(),
                    control: f[4].parse().unwrap(),
                    from: f[5].parse().unwrap(),
                    to: f[6].parse().unwrap(),
                },
                "crown" => Kind::Crown(f[3].parse().unwrap()),
                other => {
                    assert_eq!(other, "reconnect");
                    Kind::Reconnect(f[3].parse().unwrap())
                }
            };
            Ev { at, kind }
        })
        .collect()
}

type Timeline = Vec<(SystemTime, Option<f32>)>;

fn replay() -> (Vec<Revert>, Timeline) {
    let mut ledger = Ledger::default();
    let mut channels: HashMap<i64, f32> = HashMap::new();
    let mut judged: BTreeSet<SystemTime> = BTreeSet::new();
    let mut reverts = vec![];
    let mut timeline: Timeline = vec![];
    let uid = device().uid;
    let read = |c: &HashMap<i64, f32>| -> Vec<f32> { [262, 263].iter().filter_map(|k| c.get(k).copied()).collect() };

    let run_due = |ledger: &mut Ledger,
                   channels: &mut HashMap<i64, f32>,
                   judged: &mut BTreeSet<SystemTime>,
                   reverts: &mut Vec<Revert>,
                   timeline: &mut Timeline,
                   limit: Option<SystemTime>| loop {
        let (a, b) = ledger.due();
        let Some(mark) = [a, b].into_iter().flatten().filter(|d| !judged.contains(d) && limit.is_none_or(|l| *d <= l)).min()
        else {
            return;
        };
        judged.insert(mark);
        let streak_start = a.map(|x| x - JUDGE_DELAY);
        let j = ledger.judge(&device(), &read(channels), mark, false);
        if let Verdict::Revert { restore, writer, .. } = j.verdict {
            channels.insert(262, restore);
            channels.insert(263, restore);
            ledger.observe(&read(channels), &device().uid, mark);
            ledger.volume_changed(mark);
            reverts.push(Revert { at: mark, streak_start: streak_start.unwrap_or(UNIX_EPOCH), writer, restore });
        }
        timeline.push((mark, ledger.held(&device().uid)));
    };

    for ev in events() {
        run_due(&mut ledger, &mut channels, &mut judged, &mut reverts, &mut timeline, Some(ev.at));
        match ev.kind {
            Kind::Write { pid, control, from, to } => {
                let old = channels.insert(control, to);
                if old.is_none_or(|o| (o - to).abs() > 1e-9) {
                    ledger.observe(&read(&channels), &uid, ev.at);
                    ledger.volume_changed(ev.at);
                }
                let name = writer(pid);
                let listed = name == "prl_vm_app";
                let line = VolumeWrite { at: ev.at, pid, writer: name, control, from, to };
                ledger.record(line, listed, 1, Some(&uid), ev.at + Duration::from_millis(5));
            }
            Kind::Crown(v) | Kind::Reconnect(v) => {
                let changed = [262, 263].iter().any(|k| channels.get(k).is_none_or(|c| (c - v).abs() > 1e-9));
                if changed {
                    ledger.volume_changed(ev.at);
                }
                channels.insert(262, v);
                channels.insert(263, v);
                if changed {
                    ledger.observe(&read(&channels), &uid, ev.at);
                }
            }
        }
    }
    run_due(&mut ledger, &mut channels, &mut judged, &mut reverts, &mut timeline, None);
    timeline.sort_by_key(|(t, _)| *t);
    (reverts, timeline)
}

fn secs(d: Duration) -> f64 {
    d.as_secs_f64()
}

/// One write is one writer's lines within 300ms on different channels; one crown turn is clicks
/// under a second apart.
fn groups() -> Vec<Group> {
    let mut writes: Vec<Group> = vec![];
    let mut crowns: Vec<Group> = vec![];
    for ev in events() {
        match ev.kind {
            Kind::Write { pid, control, from, to } => {
                if (from - to).abs() <= 1e-6 {
                    continue;
                }
                let name = writer(pid);
                match writes.last_mut() {
                    Some(last)
                        if last.writer == name
                            && secs(ev.at.duration_since(last.end).unwrap_or_default()) < 0.3
                            && !last.channels.contains(&control) =>
                    {
                        last.end = ev.at;
                        last.to = to;
                        last.channels.insert(control);
                    }
                    _ => writes.push(Group { at: ev.at, end: ev.at, writer: name, to, channels: [control].into() }),
                }
            }
            Kind::Reconnect(_) => {}
            Kind::Crown(v) => match crowns.last_mut() {
                Some(last) if secs(ev.at.duration_since(last.end).unwrap_or_default()) < 1.0 => {
                    last.end = ev.at;
                    last.to = v;
                }
                _ => crowns.push(Group { at: ev.at, end: ev.at, writer: "Crown".into(), to: v, channels: BTreeSet::new() }),
            },
        }
    }
    let mut all = writes;
    all.extend(crowns);
    all.sort_by_key(|g| g.at);
    all
}

#[test]
fn fixture_holds_the_days_writes() {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for g in groups() {
        *counts.entry(g.writer).or_default() += 1;
    }
    let want: BTreeMap<String, usize> =
        [("prl_vm_app", 5), ("ControlCenter", 17), ("Crown", 1), ("Jetto", 50)].map(|(k, v)| (k.to_string(), v)).into();
    assert_eq!(counts, want);
}

#[test]
fn every_parallels_write_is_reverted_within_half_a_second() {
    let (reverts, _) = replay();
    let parallels: Vec<SystemTime> = groups().into_iter().filter(|g| g.writer == "prl_vm_app").map(|g| g.at).collect();
    assert_eq!(reverts.len(), 5);
    assert!(reverts.iter().all(|r| r.writer == "prl_vm_app"));
    assert_eq!(reverts.iter().map(|r| r.streak_start).collect::<Vec<_>>(), parallels);
    for r in &reverts {
        assert!((secs(r.at.duration_since(r.streak_start).unwrap()) - 0.3).abs() < 0.001);
    }
    for (got, want) in reverts.iter().map(|r| r.restore).zip([0.488189f32, 0.213605, 0.625000, 0.156250, 0.312500]) {
        assert!((got - want).abs() < 1e-6, "{got} vs {want}");
    }
}

#[test]
fn the_remembered_value_follows_everyone_else() {
    let (_, timeline) = replay();
    let groups = groups();
    let mut outcome: BTreeMap<String, usize> = BTreeMap::new();
    let mut superseded_by: Vec<String> = vec![];
    let mut echo: Vec<Group> = vec![];
    for (i, g) in groups.iter().enumerate().filter(|(_, g)| g.writer != "prl_vm_app") {
        let judged = timeline.iter().find(|(t, _)| *t >= g.end).map(|(t, _)| *t).expect("a judgement after the change");
        let held = timeline.iter().rev().find(|(t, _)| *t <= judged).and_then(|(_, h)| *h);
        if let Some(later) = groups[i + 1..].iter().find(|l| l.at < judged) {
            *outcome.entry(format!("{} superseded", g.writer)).or_default() += 1;
            superseded_by.push(format!("{}>{}", g.writer, later.writer));
            if later.writer == "prl_vm_app" {
                let after = timeline.iter().find(|(t, _)| *t >= later.at + JUDGE_DELAY).and_then(|(_, h)| *h);
                assert!((after.unwrap_or(-1.0) - g.to).abs() <= TOLERANCE);
            }
        } else if held.is_some_and(|h| (h - g.to).abs() <= TOLERANCE) {
            *outcome.entry(format!("{} held", g.writer)).or_default() += 1;
        } else {
            echo.push(g.clone());
        }
    }
    let want: BTreeMap<String, usize> = [
        ("Jetto held", 49),
        ("Jetto superseded", 1),
        ("Crown held", 1),
        ("ControlCenter held", 10),
        ("ControlCenter superseded", 6),
    ]
    .map(|(k, v)| (k.to_string(), v))
    .into();
    assert_eq!(outcome, want);
    assert_eq!(superseded_by.iter().filter(|s| *s == "ControlCenter>prl_vm_app").count(), 1);
    assert_eq!(superseded_by.iter().filter(|s| *s == "ControlCenter>ControlCenter").count(), 5);
    assert_eq!(superseded_by.iter().filter(|s| *s == "Jetto>Jetto").count(), 1);
    assert_eq!(echo.len(), 1);
    assert_eq!(echo[0].writer, "ControlCenter");
    assert!((echo[0].to - 0.5625).abs() < 1e-6);
    let held = timeline.iter().rev().find(|(t, _)| *t <= echo[0].end + Duration::from_millis(300)).and_then(|(_, h)| *h);
    assert!((held.unwrap_or(0.0) - 0.625).abs() < 1e-6);
}
