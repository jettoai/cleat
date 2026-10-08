//! scripts/install.sh writes the KeepAlive that lets the menu's Quit stay down (B-1222).

#[test]
fn install_sets_keepalive_successful_exit_false() {
    let script = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/install.sh")).unwrap();
    let line = r#"plutil -replace KeepAlive -json '{"SuccessfulExit":false}' "$PLIST""#;
    assert!(script.lines().any(|l| l.trim() == line), "install.sh lacks: {line}");
}
