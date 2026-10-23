//! The engine's side of the HEIC converter: when it starts, restarts and stops the watch, and the
//! log lines it writes for the worker's reports.

mod common;

use cleat_rs::config::{Config, HeicConfig};
use cleat_rs::engine::{Event, Mode};
use cleat_rs::model::DeviceSnapshot;
use cleat_rs::photos::Report;
use common::engine::{Harness, Opts};

fn run(json: &str, opts: Opts) -> Harness {
    Harness::with_json(json, DeviceSnapshot::default(), opts)
}

fn touch(h: &mut Harness, c: &Config) {
    std::fs::write(h.dir.join("config.json"), serde_json::to_string(c).unwrap()).unwrap();
    h.engine.handle(Event::ConfigTouched { received: std::time::SystemTime::now() });
    h.drive(400);
}

fn config(convert: bool, long_edge: u32) -> Config {
    Config {
        launch_at_login: false,
        convert_heic: convert,
        heic: HeicConfig { long_edge, ..HeicConfig::default() },
        ..Config::default()
    }
}

#[test]
fn off_never_calls_the_start_callback_and_writes_no_line() {
    let h = run(r#"{"launchAtLogin": false}"#, Opts::default());
    assert!(h.photos.borrow().is_empty());
    let h = run(r#"{"launchAtLogin": false, "convertHeic": false, "heic": {"longEdge": 1600}}"#, Opts::default());
    assert!(h.photos.borrow().is_empty());
    assert_eq!(h.count("heic:"), 0);
    let h = run(r#"{"launchAtLogin": false, "convertHeic": true}"#, Opts { mode: Mode::Observe, ..Opts::default() });
    assert!(h.photos.borrow().is_empty(), "observe mode starts nothing");
}

#[test]
fn on_starts_the_watch_with_the_settings_and_follows_the_config_both_ways() {
    let mut h = run(r#"{"launchAtLogin": false, "convertHeic": true, "heic": {"watch": "~/Pictures"}}"#, Opts::default());
    let first = HeicConfig { watch: "~/Pictures".into(), ..HeicConfig::default() };
    assert_eq!(*h.photos.borrow(), vec![Some(first.clone())]);
    assert_eq!(h.count("heic: watching ~/Pictures"), 1);

    // An unrelated change does not restart it.
    let mut same = config(true, 2400);
    same.heic = first.clone();
    same.balance = Some(0.5);
    touch(&mut h, &same);
    assert_eq!(h.photos.borrow().len(), 1);

    touch(&mut h, &config(true, 3200));
    touch(&mut h, &config(false, 3200));
    touch(&mut h, &config(false, 1600)); // off stays off
    let calls = h.photos.borrow().clone();
    assert_eq!(calls.len(), 3);
    assert_eq!(calls[1].as_ref().map(|c| c.long_edge), Some(3200));
    assert_eq!(calls[2], None);
    assert_eq!(h.count("heic: off"), 1);
}

#[test]
fn the_workers_reports_become_log_lines() {
    let mut h = run(r#"{"launchAtLogin": false}"#, Opts::default());
    let converted = Report::Converted {
        from: "IMG_5708.HEIC".into(),
        to: "IMG_5708.jpg".into(),
        width: 1800,
        height: 2400,
        ms: 187,
        originals: "HEIC原檔".into(),
    };
    h.engine.handle(Event::Photos(converted));
    h.engine.handle(Event::Photos(Report::NoAccess { path: "~/Downloads".into() }));
    h.engine.handle(Event::Photos(Report::Failed { name: "BAD.HEIC".into(), reason: "cannot decode the picture".into() }));
    let lines = h.log_lines();
    let tail = |n: usize| lines[lines.len() - n].split_once(' ').map(|(_, rest)| rest.to_string()).unwrap();
    assert_eq!(tail(3).split_once(' ').unwrap().1, "heic: IMG_5708.HEIC -> IMG_5708.jpg 1800x2400 187 ms, original -> HEIC原檔/");
    assert_eq!(tail(2).split_once(' ').unwrap().1, "heic: no access to ~/Downloads");
    assert_eq!(tail(1).split_once(' ').unwrap().1, "heic: failed BAD.HEIC: cannot decode the picture");
}

fn photos_block(h: &Harness) -> serde_json::Value {
    let status: serde_json::Value = serde_json::from_slice(&std::fs::read(h.dir.join("status.json")).unwrap()).unwrap();
    status["photos"].clone()
}

fn converted(name: &str, ms: u128) -> Report {
    Report::Converted { from: name.into(), to: "x.jpg".into(), width: 1, height: 1, ms, originals: "HEIC原檔".into() }
}

/// status.json `photos`: off, then watching with the count and the last picture, then no-access
/// until the folder can be read again, then off again keeping nothing but the state.
#[test]
fn status_json_carries_the_photos_block() {
    let mut h = run(r#"{"launchAtLogin": false}"#, Opts::default());
    assert_eq!(photos_block(&h), serde_json::json!({"state": "off", "converted": 0}));

    touch(&mut h, &config(true, 2400));
    assert_eq!(photos_block(&h), serde_json::json!({"state": "watching", "converted": 0}));

    h.engine.handle(Event::Photos(converted("IMG_5708.HEIC", 187)));
    h.engine.handle(Event::Photos(converted("IMG_5709.HEIC", 201)));
    assert_eq!(
        photos_block(&h),
        serde_json::json!({"state": "watching", "converted": 2, "last": {"name": "IMG_5709.HEIC", "ms": 201}})
    );

    h.engine.handle(Event::Photos(Report::NoAccess { path: "~/Downloads".into() }));
    assert_eq!(photos_block(&h)["state"], "no-access");
    h.engine.handle(Event::Photos(Report::Readable { path: "~/Downloads".into() }));
    assert_eq!(photos_block(&h)["state"], "watching");

    h.engine.handle(Event::Photos(Report::NoAccess { path: "~/Downloads".into() }));
    touch(&mut h, &config(false, 2400));
    assert_eq!(photos_block(&h)["state"], "off");
    touch(&mut h, &config(true, 2400));
    assert_eq!(photos_block(&h)["state"], "watching", "a new watch starts clear of the old one's trouble");
}
