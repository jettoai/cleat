mod common;

use std::path::{Path, PathBuf};

use cleat_rs::config::paths::{config_path_from, log_dir, log_path, status_path, support_dir};
use cleat_rs::config::{Config, ConfigError, LivenessConfig};
use common::*;

fn decode(json: &str) -> Result<Config, serde_json::Error> {
    serde_json::from_str(json)
}

fn liveness(entry: &str, zero: f64) -> std::collections::BTreeMap<String, LivenessConfig> {
    [(entry.to_string(), LivenessConfig { zero_seconds: zero })].into_iter().collect()
}

fn temp_path(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!("cleat-{tag}-{}-{nanos}.json", std::process::id()))
}

#[test]
fn error_reports_are_off_unless_asked() {
    assert!(!decode("{}").unwrap().error_reports);
    assert!(decode(r#"{"errorReports": true}"#).unwrap().error_reports);
    assert!(!decode(r#"{"errorReports": false}"#).unwrap().error_reports);
    assert!(decode(r#"{"errorReports": "yes"}"#).is_err());
}

#[test]
fn full_config_decodes() {
    let config = decode(
        r#"{
          "input": ["Wireless microphone", "Brio 100"],
          "blockedInput": ["AirPods Max"],
          "output": ["Studio Display Speakers"],
          "blockedOutput": ["Maono AI Microphone"],
          "headphonesTakeOver": true,
          "balance": 0.5,
          "inputVolume": { "Wireless microphone": 88, "Brio 100": 75 },
          "liveness": { "Wireless microphone": { "zeroSeconds": 3 } },
          "reclaim": ["AirPods Max"],
          "launchAtLogin": false
        }"#,
    )
    .unwrap();
    assert_eq!(config.input, s(&["Wireless microphone", "Brio 100"]));
    assert_eq!(config.blocked_input, s(&["AirPods Max"]));
    assert_eq!(config.output, s(&["Studio Display Speakers"]));
    assert_eq!(config.blocked_output, s(&["Maono AI Microphone"]));
    assert!(config.headphones_take_over);
    assert_eq!(config.balance, Some(0.5));
    assert_eq!(config.input_volume, volumes(&[("Wireless microphone", 88.0), ("Brio 100", 75.0)]));
    assert_eq!(config.liveness, liveness("Wireless microphone", 3.0));
    assert_eq!(config.reclaim, s(&["AirPods Max"]));
    assert!(!config.launch_at_login);
    assert!(config.validate().is_ok());
}

#[test]
fn missing_fields_fall_back_to_defaults() {
    let config = decode("{}").unwrap();
    assert_eq!(config, Config::disabled());
    assert_eq!(config.input, Vec::<String>::new());
    assert_eq!(config.blocked_input, Vec::<String>::new());
    assert_eq!(config.output, Vec::<String>::new());
    assert_eq!(config.blocked_output, Vec::<String>::new());
    assert!(!config.headphones_take_over);
    assert!(config.balance.is_none());
    assert_eq!(config.input_volume, volumes(&[]));
    assert_eq!(config.liveness, Default::default());
    assert_eq!(config.reclaim, Vec::<String>::new());
    assert!(config.launch_at_login);
}

#[test]
fn partial_config_keeps_other_defaults() {
    let config = decode(r#"{"input": ["Brio 100"]}"#).unwrap();
    assert_eq!(config.input, s(&["Brio 100"]));
    assert!(config.balance.is_none());
    assert!(config.launch_at_login);
}

#[test]
fn headphones_take_over_decodes_on_its_own() {
    let config = decode(r#"{"headphonesTakeOver": true}"#).unwrap();
    assert!(config.headphones_take_over);
    assert_eq!(config.output, Vec::<String>::new());
    assert!(config.validate().is_ok());
}

#[test]
fn malformed_json_throws() {
    assert!(decode("{ this is not json ").is_err());
}

#[test]
fn wrong_type_throws() {
    assert!(decode(r#"{"input": "Brio 100"}"#).is_err());
}

#[test]
fn balance_out_of_range_fails_validation() {
    let config = decode(r#"{"balance": 1.5}"#).unwrap();
    assert_eq!(config.validate(), Err(ConfigError::BalanceOutOfRange(1.5)));
}

#[test]
fn negative_balance_fails_validation() {
    let config = decode(r#"{"balance": -0.2}"#).unwrap();
    assert!(config.validate().is_err());
}

#[test]
fn volume_out_of_range_fails_validation() {
    let config = decode(r#"{"inputVolume": {"Brio 100": 140}}"#).unwrap();
    assert_eq!(config.validate(), Err(ConfigError::VolumeOutOfRange { device: "Brio 100".into(), value: 140.0 }));
}

#[test]
fn zero_seconds_below_one_fails_validation() {
    let config = decode(r#"{"liveness": {"Brio 100": {"zeroSeconds": 0.5}}}"#).unwrap();
    assert_eq!(config.validate(), Err(ConfigError::ZeroSecondsTooSmall { device: "Brio 100".into(), value: 0.5 }));
}

#[test]
fn wildcard_volume_decodes() {
    let config = decode(r#"{"inputVolume": {"*": 100, "Brio 100": 75}}"#).unwrap();
    assert_eq!(config.input_volume, volumes(&[("*", 100.0), ("Brio 100", 75.0)]));
    assert!(config.input_volume_has_wildcard());
    assert!(config.validate().is_ok());
}

#[test]
fn wildcard_out_of_range_fails_validation() {
    let config = decode(r#"{"inputVolume": {"*": 140}}"#).unwrap();
    assert_eq!(config.validate(), Err(ConfigError::VolumeOutOfRange { device: "*".into(), value: 140.0 }));
}

#[test]
fn named_target_wins_over_wildcard() {
    let config = decode(r#"{"inputVolume": {"*": 100, "Brio 100": 75}}"#).unwrap();
    assert_eq!(config.input_volume_target(&brio()), Some(75.0));
}

#[test]
fn unnamed_device_takes_the_wildcard_target() {
    let config = decode(r#"{"inputVolume": {"*": 100, "Brio 100": 75}}"#).unwrap();
    assert_eq!(config.input_volume_target(&air_pods()), Some(100.0));
}

#[test]
fn uid_entry_wins_over_wildcard() {
    let uid = air_pods().uid;
    let config = Config { input_volume: volumes(&[("*", 100.0), (uid.as_str(), 60.0)]), ..Config::default() };
    assert_eq!(config.input_volume_target(&air_pods()), Some(60.0));
}

#[test]
fn without_wildcard_an_unnamed_device_has_no_target() {
    let config = decode(r#"{"inputVolume": {"Brio 100": 75}}"#).unwrap();
    assert!(!config.input_volume_has_wildcard());
    assert!(config.input_volume_target(&air_pods()).is_none());
    assert_eq!(config.input_volume_target(&brio()), Some(75.0));
}

#[test]
fn empty_input_volume_has_no_targets() {
    assert!(!Config::disabled().input_volume_has_wildcard());
    assert!(Config::disabled().input_volume_target(&brio()).is_none());
}

#[test]
fn both_devices_sharing_a_name_are_worth_reading() {
    let config = Config { input_volume: volumes(&[("Brio 100", 75.0)]), ..Config::default() };
    assert_eq!(config.input_volume_devices(&[brio(), second_brio(), wireless()]), vec![brio(), second_brio()]);
}

#[test]
fn only_input_devices_with_a_target_are_worth_reading() {
    let named = Config { input_volume: volumes(&[("Brio 100", 75.0)]), ..Config::default() };
    assert_eq!(named.input_volume_devices(&[brio(), display_speakers(), wireless()]), vec![brio()]);

    let wildcard = Config { input_volume: volumes(&[("*", 100.0)]), ..Config::default() };
    assert_eq!(wildcard.input_volume_devices(&[brio(), display_speakers(), wireless()]), vec![brio(), wireless()]);

    assert_eq!(Config::disabled().input_volume_devices(&[brio()]), vec![]);
}

#[test]
fn load_from_file_round_trips() {
    let path = temp_path("config");
    std::fs::write(&path, r#"{"input": ["Brio 100"], "balance": 0.5}"#).unwrap();
    let config = Config::load(&path);
    let _ = std::fs::remove_file(&path);
    let config = config.unwrap();
    assert_eq!(config.input, s(&["Brio 100"]));
    assert_eq!(config.balance, Some(0.5));
}

#[test]
fn load_missing_file_throws() {
    assert!(Config::load(&temp_path("absent")).is_err());
}

// The Swift tests set CLEAT_CONFIG in the process environment; cargo runs tests in parallel, so
// these call the pure resolver with the same inputs instead.

#[test]
fn config_path_falls_back_to_the_user_config_directory() {
    let home = Path::new("/Users/someone");
    assert_eq!(config_path_from(home, None), home.join(".config/cleat").join("config.json"));
}

#[test]
fn config_path_honours_the_environment_override() {
    let home = Path::new("/Users/someone");
    assert_eq!(config_path_from(home, Some("/tmp/cleat-probe/config.json")), PathBuf::from("/tmp/cleat-probe/config.json"));
    assert_eq!(status_path(), support_dir().join("status.json"));
    assert_eq!(log_path(), log_dir().join("cleat.log"));
}

#[test]
fn config_path_ignores_an_empty_override() {
    let home = Path::new("/Users/someone");
    assert_eq!(config_path_from(home, Some("")), home.join(".config/cleat").join("config.json"));
}

#[test]
fn output_volume_hold_is_off_unless_asked() {
    assert!(decode("{}").unwrap().output_volume_hold_against.is_empty());
    assert_eq!(
        decode(r#"{"outputVolumeHoldAgainst": ["prl_vm_app"]}"#).unwrap().output_volume_hold_against,
        s(&["prl_vm_app"])
    );
    assert!(decode(r#"{"outputVolumeHoldAgainst": "prl_vm_app"}"#).is_err());
}

/// §6.6 E1-E3: the switches default on and turn a rule off without emptying its list.
#[test]
fn switches_are_kept_apart_from_their_lists() {
    let c = decode("{}").unwrap();
    assert!(c.reclaim_enabled && c.output_volume_hold_enabled);
    let c = decode(r#"{"reclaimEnabled": false, "reclaim": ["AirPods Max"]}"#).unwrap();
    assert_eq!(c.reclaim, s(&["AirPods Max"]));
    assert!(c.reclaim_active().is_empty());
    let c = decode(r#"{"outputVolumeHoldEnabled": false, "outputVolumeHoldAgainst": ["Parallels Desktop"]}"#).unwrap();
    assert_eq!(c.output_volume_hold_against, s(&["Parallels Desktop"]));
    assert!(c.hold_against_active().is_empty());
    assert!(decode(r#"{"reclaimEnabled": "no"}"#).is_err());
}
