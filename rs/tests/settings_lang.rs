//! B-1283: the window and the menu speak Traditional Chinese on a Traditional Chinese system and
//! English everywhere else. English has no CJK in it; Chinese reads as it did at e146bbd
//! (`fixtures/zh_hant_text.txt`, rendered by that tree); no CJK literal lives outside `words.rs`.

use cleat_rs::app::menubar::device_lines;
use cleat_rs::engine::RevertDetail;
use cleat_rs::settings::document::WriteError;
use cleat_rs::settings::draft::{HeadsetBox, ReclaimHint, Side};
use cleat_rs::settings::lang::{from_preferred, Lang};
use cleat_rs::settings::sources::LastRevert;
use cleat_rs::settings::text::*;
use cleat_rs::settings::vitals::{DaemonVitals, VitalsState};
use cleat_rs::settings::without_defaults_args;
use cleat_rs::settings::words::W;
use cleat_rs::state::reaction_clock::DaemonPerformance;

fn is_cjk(c: char) -> bool {
    matches!(c as u32, 0x3000..=0x303F | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0xFF00..=0xFFEF)
}

/// Every templated sentence, with the sample values the e146bbd fixture was rendered with.
fn renders(l: Lang) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = vec![];
    let mut put = |k: &str, v: String| out.push((k.into(), v));
    for v in [0.5, 0.49, 0.4, 0.0, 1.0, 0.75] {
        put(&format!("balance:{v}"), balance_describe(v, l));
    }
    put("now:none:out", now_text(None, None, Side::Output, l));
    put("now:none:in", now_text(None, None, Side::Input, l));
    put("now:noread", now_text(Some("Mic"), None, Side::Input, l));
    put("now:read", now_text(Some("Mic"), Some("62%"), Side::Input, l));
    for (sn, s, notset, prio, others) in [
        ("out", Side::Output, W::NotSetUpOutput, W::PriorityOutput, W::OthersOutput),
        ("in", Side::Input, W::NotSetUpInput, W::PriorityInput, W::OthersInput),
    ] {
        put(&format!("stuck:{sn}"), stuck_header(s, Some("AirPods Max"), None, l).join("\n"));
        put(&format!("paused:{sn}"), stuck_header(s, None, Some("AirPods Max"), l).join("\n"));
        put(&format!("footer:{sn}"), device_list_footer(s, l).into());
        put(&format!("notsetup:{sn}"), notset.get(l).into());
        put(&format!("priority:{sn}"), prio.get(l).into());
        put(&format!("others:{sn}"), others.get(l).into());
    }
    put("headset:bt", headset_note(HeadsetBox::BlockedTicked, true, l).unwrap_or("-").into());
    put("headset:bo", headset_note(HeadsetBox::BlockedOff, true, l).unwrap_or("-").into());
    put("headset:nc", headset_note(HeadsetBox::Plain(true), false, l).unwrap_or("-").into());
    for (n, h) in [("none", ReclaimHint::NoHeadsets), ("all", ReclaimHint::AllHeadsetsBlocked), ("tick", ReclaimHint::NoneTicked), ("tb", ReclaimHint::AllTickedBlocked)] {
        put(&format!("hint:{n}"), reclaim_hint_text(h, l).into());
    }
    for (n, s) in [("run", VitalsState::Running), ("not", VitalsState::NotRunning), ("unr", VitalsState::Unreadable)] {
        put(&format!("headline:{n}"), vitals_headline(s, l).into());
    }
    let measuring = DaemonVitals { cpu_measuring: true, ..DaemonVitals::with_state(VitalsState::Running) };
    put("cpu:measuring", vitals_cpu(&measuring, l));
    put("cpunote:none", vitals_cpu_note(&measuring, l));
    put("cpunote:60", vitals_cpu_note(&DaemonVitals { cpu_window_seconds: Some(60), ..measuring.clone() }, l));
    let perf = DaemonPerformance { samples: 2, median_reaction_ms: Some(3.0), last_reaction_ms: Some(3.5), last_work_ms: Some(0.4), ..Default::default() };
    let run = |p: Option<DaemonPerformance>| DaemonVitals { performance: p, ..DaemonVitals::with_state(VitalsState::Running) };
    put("reaction:old:note", reaction_note(&run(None), l));
    put("reaction:old:help", reaction_help(&run(None), l));
    put("reaction:fresh:help", reaction_help(&run(Some(DaemonPerformance::default())), l));
    put("reaction:some:note", reaction_note(&run(Some(perf.clone())), l));
    put("reaction:some:help", reaction_help(&run(Some(perf.clone())), l));
    put("reaction:stopped:help", reaction_help(&DaemonVitals { performance: Some(perf), ..DaemonVitals::with_state(VitalsState::NotRunning) }, l));
    put("write:obj", WriteError::NotAnObject.message(l));
    put("write:changed", WriteError::ChangedOnDisk.message(l));
    put("write:invalid", WriteError::Invalid("x".into()).message(l));
    put("write:io", WriteError::Io("x".into()).message(l));
    put("unreadable", fill(W::Unreadable.get(l), &["x"]));
    put("otherbt", fill(W::OtherBluetooth.get(l), &["3"]));
    put("nobalance", fill(W::NoBalance.get(l), &["MacBook Pro Speakers"]));
    put("revert:legacy", revert_line(Some(&LastRevert::Legacy("13:40 x".into())), l));
    let [o, i] = device_lines(Some(&status_with("AirPods Max")), l);
    put("menu:out", o);
    put("menu:in", i);
    put("launchfail", fill(W::LaunchRegisterFailed.get(l), &["x"]));
    out
}

fn status_with(name: &str) -> cleat_rs::engine::Status {
    let mut s: cleat_rs::engine::Status = serde_json::from_str(
        r#"{"pid":1,"updatedAt":"","configState":"","microphone":"","rules":{},"liveness":{},"recentEvents":[]}"#,
    )
    .unwrap();
    s.default_output = Some(name.into());
    s.default_input = Some(name.into());
    s
}

/// Branches the fixture predates: the structured revert, the legacy line without a clock, tags.
fn english_only_renders() -> Vec<String> {
    let l = Lang::En;
    let detail = RevertDetail { at: "13:40".into(), writer: "Parallels Desktop".into(), from: 43.0, to: 50.0 };
    let mut v = vec![
        revert_line(None, l),
        revert_line(Some(&LastRevert::Detail(detail)), l),
        revert_line(Some(&LastRevert::Legacy("拉回".into())), l),
        device_lines(None, l).join("\n"),
    ];
    use cleat_rs::settings::draft::{DeviceRow, Stance};
    let r = DeviceRow { entry: "Mic".into(), display_name: "Mic".into(), is_connected: true, stance: Stance::Blocked };
    for reason in [RowReason::Stuck, RowReason::Paused, RowReason::None] {
        v.extend(device_row_tags(&r, Some("Mic"), reason, l).into_iter().map(|(t, _)| t.to_string()));
    }
    v
}

#[test]
fn english_has_no_cjk() {
    for w in W::ALL {
        assert!(!w.en().chars().any(is_cjk), "{w:?}: {}", w.en());
    }
    for (k, v) in renders(Lang::En) {
        assert!(!v.chars().any(is_cjk), "{k}: {v}");
    }
    for v in english_only_renders() {
        assert!(!v.chars().any(is_cjk), "{v}");
    }
    assert_eq!(revert_line(Some(&LastRevert::Legacy("13:40 拉回 x 改的音量 43% → 50%".into())), Lang::En), "Last undo at 13:40");
}

fn fixture() -> (Vec<(String, String)>, Vec<String>) {
    let text = include_str!("fixtures/zh_hant_text.txt");
    let (mut rendered, mut literals) = (vec![], vec![]);
    for line in text.lines() {
        let (k, v) = line.split_once('\t').unwrap();
        let v = v.replace("\\n", "\n");
        if k == "lit" { literals.push(v) } else { rendered.push((k.to_string(), v)) }
    }
    (rendered, literals)
}

#[test]
fn zh_hant_unchanged() {
    let (want, literals) = fixture();
    assert_eq!(renders(Lang::ZhHant), want);
    // A word with no placeholder was a literal at e146bbd, or a sentence the fixture rendered.
    for w in W::ALL {
        let zh = w.zh();
        if zh.contains("{}") {
            continue;
        }
        assert!(literals.iter().any(|l| l == zh) || want.iter().any(|(_, v)| v == zh), "{w:?}: {zh} is not what e146bbd showed");
    }
}

#[test]
fn no_cjk_outside_words() {
    let allowed = [("src/settings/text.rs", "耳機"), ("src/settings/text.rs", "顯示器"), ("src/engine/output_volume.rs", "{} 拉回 {} 改的音量 {:.0}% → {:.0}%")];
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut found = vec![];
    let mut dirs = vec![root.join("src")];
    while let Some(d) = dirs.pop() {
        for e in std::fs::read_dir(&d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                dirs.push(p);
                continue;
            }
            let rel = p.strip_prefix(root).unwrap().to_string_lossy().to_string();
            if !rel.ends_with(".rs") || rel == "src/settings/words.rs" {
                continue;
            }
            for line in std::fs::read_to_string(&p).unwrap().lines() {
                if line.trim_start().starts_with("#[cfg(test)]") {
                    break;
                }
                if line.trim_start().starts_with("//") {
                    continue;
                }
                for s in string_literals(line) {
                    if s.chars().any(is_cjk) && !allowed.contains(&(rel.as_str(), s.as_str())) {
                        found.push(format!("{rel}: {s}"));
                    }
                }
            }
        }
    }
    assert!(found.is_empty(), "words outside words.rs: {found:#?}");
}

/// The census regex `"([^"\\]*(?:\\.[^"\\]*)*)"`.
fn string_literals(line: &str) -> Vec<String> {
    let (mut out, mut cur, mut inside, mut esc) = (vec![], String::new(), false, false);
    for c in line.chars() {
        if !inside {
            inside = c == '"';
        } else if esc {
            cur.push(c);
            esc = false;
        } else if c == '\\' {
            cur.push(c);
            esc = true;
        } else if c == '"' {
            out.push(std::mem::take(&mut cur));
            inside = false;
        } else {
            cur.push(c);
        }
    }
    out
}

#[test]
fn language_from_preferred() {
    let one = |s: &str| from_preferred(&[s.to_string()]);
    for s in ["zh-Hant-TW", "zh-TW", "zh_TW", "zh-Hant-HK", "zh-HK", "zh-MO", "zh-Hant"] {
        assert_eq!(one(s), Lang::ZhHant, "{s}");
    }
    for s in ["zh-Hans-CN", "zh-CN", "zh", "zh-Hantx", "en-US", "en-TW", "ja-JP", "ko", ""] {
        assert_eq!(one(s), Lang::En, "{s}");
    }
    assert_eq!(from_preferred(&[]), Lang::En);
    assert_eq!(from_preferred(&["ja".into(), "zh-Hant-TW".into()]), Lang::En, "only the first counts");
}

#[test]
fn placeholders_line_up() {
    for w in W::ALL {
        assert_eq!(w.zh().matches("{}").count(), w.en().matches("{}").count(), "{w:?}");
    }
}

#[test]
fn settings_ignores_defaults_args() {
    let v = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert!(without_defaults_args(&v(&["-AppleLanguages", "(en)"])).is_empty());
    assert_eq!(without_defaults_args(&v(&["-NSDocumentRevisionsDebugMode", "YES", "x"])), v(&["x"]));
    assert_eq!(without_defaults_args(&v(&["x"])), v(&["x"]));
    assert_eq!(without_defaults_args(&v(&["-A"])), v(&["-A"]));
}
