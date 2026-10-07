use cleat_rs::model::device_name::{matches, normalize};

#[test]
fn no_break_space_matches_plain_space() {
    assert!(matches("Maono AI Microphone", "Maono\u{00A0}AI Microphone", "AppleUSBAudioEngine:Maono:1"));
}

#[test]
fn narrow_no_break_space_matches_plain_space() {
    assert!(matches("Maono AI Microphone", "Maono\u{202F}AI Microphone", "uid"));
}

#[test]
fn different_case_does_not_match() {
    assert!(!matches("brio 100", "Brio 100", "uid"));
}

#[test]
fn exact_uid_matches() {
    let uid = "AppleUSBAudioEngine:Shenzhen Hollyland Technology Co.,Ltd:Wireless microphone:952X2D2Q952:3";
    assert!(matches(uid, "Wireless microphone", uid));
}

#[test]
fn uid_is_compared_literally_not_normalized() {
    assert!(!matches("uid:a", "Some Device", "uid:b"));
}

#[test]
fn surrounding_whitespace_is_trimmed() {
    assert!(matches("  Brio 100  ", "Brio 100", "uid"));
}

#[test]
fn repeated_spaces_collapse() {
    assert!(matches("Brio    100", "Brio 100", "uid"));
}

#[test]
fn empty_entry_does_not_match_empty_uid() {
    assert!(!matches("", "Brio 100", ""));
}

#[test]
fn normalize_is_idempotent() {
    let once = normalize("Maono\u{00A0}AI  Microphone ");
    assert_eq!(once, "Maono AI Microphone");
    assert_eq!(normalize(&once), once);
}

#[test]
fn unrelated_name_does_not_match() {
    assert!(!matches("Wireless microphone", "Brio 100", "uid"));
}
