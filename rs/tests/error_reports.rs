//! ErrorReportingTests `testScrubFoldsTheHomeDirectoryAndDropsTheUser` (Swift :7), on the Rust
//! event shape, plus the envelope framing the transport posts.

use cleat_rs::report::event::{self, Meta};
use serde_json::json;

#[test]
fn scrub_folds_the_home_directory_and_drops_the_user() {
    let home = "/Users/someone";
    let meta = Meta::default();
    let frames = vec![json!({"package": format!("{home}/Applications/Cleat.app/Contents/MacOS/Cleat"), "filename": format!("{home}/src/x.rs"), "module": format!("{home}/m")})];
    let mut e = event::exception(&meta, 0.0, "EXC_BAD_ACCESS", &format!("crash in {home}/Applications/Cleat.app"), frames);
    e["user"] = json!({"id": "installation"});
    e["message"] = json!({"formatted": format!("failed to read {home}/.config/cleat/config.json")});
    e["debug_meta"] = json!({"images": [{"code_file": format!("{home}/Applications/Cleat.app/Contents/MacOS/Cleat")}]});

    event::scrub(&mut e, home);

    assert!(e.get("user").is_none());
    assert_eq!(e["message"]["formatted"], "failed to read ~/.config/cleat/config.json");
    let exc = &e["exception"]["values"][0];
    assert_eq!(exc["value"], "crash in ~/Applications/Cleat.app");
    let frame = &exc["stacktrace"]["frames"][0];
    assert_eq!(frame["package"], "~/Applications/Cleat.app/Contents/MacOS/Cleat");
    assert_eq!(frame["filename"], "~/src/x.rs");
    assert_eq!(frame["module"], "~/m");
    assert_eq!(e["debug_meta"]["images"][0]["code_file"], "~/Applications/Cleat.app/Contents/MacOS/Cleat");
    assert!(!e.to_string().contains(home));
}

#[test]
fn envelope_is_three_lines_with_a_matching_length() {
    let e = event::message(&Meta::default(), "info", 1.0, "probe");
    let bytes = event::envelope(&e, "2026-10-07T00:00:00Z");
    let text = String::from_utf8(bytes).unwrap();
    let lines: Vec<&str> = text.trim_end().split('\n').collect();
    assert_eq!(lines.len(), 3);
    let header: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    let item: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
    assert_eq!(header["event_id"], e["event_id"]);
    assert_eq!(item["type"], "event");
    assert_eq!(item["length"].as_u64().unwrap() as usize, lines[2].len());
    assert_eq!(e["event_id"].as_str().unwrap().len(), 32);
}
