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
fn usage_ping_is_on_unless_switched_off() {
    assert!(decode("{}").unwrap().usage_ping);
    assert!(decode(r#"{"usagePing": true}"#).unwrap().usage_ping);
    assert!(!decode(r#"{"usagePing": false}"#).unwrap().usage_ping);
    assert!(decode(r#"{"usagePing": "no"}"#).is_err());
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
fn remember_displays_is_off_unless_asked() {
    assert!(!decode("{}").unwrap().remember_displays);
    assert!(decode(r#"{"rememberDisplays": true}"#).unwrap().remember_displays);
    assert!(decode(r#"{"rememberDisplays": "yes"}"#).is_err());
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
    assert!(c.reclaim_enabled.is_none() && c.output_volume_hold_enabled);
    // B-1287: the window's switch left on with nothing ticked; the daemon has nothing to ask for.
    let c = decode(r#"{"reclaimEnabled": true, "reclaim": []}"#).unwrap();
    assert!(c.validate().is_ok() && c.reclaim_active().is_empty());
    let c = decode(r#"{"reclaimEnabled": false, "reclaim": ["AirPods Max"]}"#).unwrap();
    assert_eq!(c.reclaim, s(&["AirPods Max"]));
    assert!(c.reclaim_active().is_empty());
    let c = decode(r#"{"outputVolumeHoldEnabled": false, "outputVolumeHoldAgainst": ["Parallels Desktop"]}"#).unwrap();
    assert_eq!(c.output_volume_hold_against, s(&["Parallels Desktop"]));
    assert!(c.hold_against_active().is_empty());
    assert!(decode(r#"{"reclaimEnabled": "no"}"#).is_err());
}

#[test]
fn convert_heic_is_off_and_the_heic_block_has_its_defaults_when_the_keys_are_absent() {
    let config = decode("{}").unwrap();
    assert!(!config.convert_heic);
    assert_eq!(config.heic.watch, "~/Downloads");
    assert_eq!(config.heic.originals, "HEIC原檔");
    assert_eq!((config.heic.long_edge, config.heic.quality), (1200, 60));
    assert_eq!(config.validate(), Ok(()));
}

#[test]
fn a_heic_block_may_leave_out_any_field() {
    let config = decode(r#"{"convertHeic": true, "heic": {"longEdge": 2400}}"#).unwrap();
    assert!(config.convert_heic);
    assert_eq!(config.heic.long_edge, 2400);
    assert_eq!((config.heic.watch.as_str(), config.heic.originals.as_str(), config.heic.quality), ("~/Downloads", "HEIC原檔", 60));
}

#[test]
fn a_config_from_before_heic_reads_as_it_did() {
    let config = decode(r#"{"output": ["AirPods Max"], "balance": 0.5, "launchAtLogin": false}"#).unwrap();
    assert_eq!(config.output, ["AirPods Max"]);
    assert!(!config.convert_heic);
    assert_eq!(config.heic, Default::default());
    assert_eq!(config.validate(), Ok(()));
}

#[test]
fn heic_long_edge_is_zero_or_800_to_8000() {
    let check = |v: u32| decode(&format!(r#"{{"heic": {{"longEdge": {v}}}}}"#)).unwrap().validate();
    for ok in [0, 800, 2400, 8000] {
        assert_eq!(check(ok), Ok(()), "{ok}");
    }
    for bad in [1, 799, 8001] {
        assert_eq!(check(bad), Err(ConfigError::HeicLongEdgeOutOfRange(bad)), "{bad}");
    }
    assert_eq!(
        check(799).unwrap_err().to_string(),
        "heic.longEdge must be 0 or between 800 and 8000, got 799"
    );
}

#[test]
fn heic_quality_is_50_to_100() {
    let check = |v: u32| decode(&format!(r#"{{"heic": {{"quality": {v}}}}}"#)).unwrap().validate();
    for ok in [50, 85, 100] {
        assert_eq!(check(ok), Ok(()), "{ok}");
    }
    for bad in [0, 49, 101] {
        assert_eq!(check(bad), Err(ConfigError::HeicQualityOutOfRange(bad)), "{bad}");
    }
    assert_eq!(check(101).unwrap_err().to_string(), "heic.quality must be between 50 and 100, got 101");
    assert!(decode(r#"{"heic": {"quality": -1}}"#).is_err());
}

#[test]
fn a_file_with_an_out_of_range_heic_value_is_refused_whole() {
    let path = temp_path("heic-range");
    std::fs::write(&path, r#"{"output": ["A"], "heic": {"quality": 49}}"#).unwrap();
    let loaded = Config::load(&path);
    let _ = std::fs::remove_file(&path);
    assert!(matches!(loaded, Err(cleat_rs::config::LoadError::Invalid(ConfigError::HeicQualityOutOfRange(49)))));
}

#[test]
fn heic_paths_expand_the_tilde_and_a_bare_originals_name_lives_in_the_watched_folder() {
    let home = Path::new("/Users/someone");
    let heic = decode(r#"{"heic": {"watch": "~/Pictures/in", "originals": "Originals"}}"#).unwrap().heic;
    assert_eq!(heic.watch_dir(home), Path::new("/Users/someone/Pictures/in"));
    assert_eq!(heic.originals_dir(home), Path::new("/Users/someone/Pictures/in/Originals"));
    let elsewhere = decode(r#"{"heic": {"originals": "~/Archive"}}"#).unwrap().heic;
    assert_eq!(elsewhere.originals_dir(home), Path::new("/Users/someone/Archive"));
    let absolute = decode(r#"{"heic": {"watch": "/Volumes/in", "originals": "/Volumes/out"}}"#).unwrap().heic;
    assert_eq!((absolute.watch_dir(home).as_path(), absolute.originals_dir(home).as_path()), (Path::new("/Volumes/in"), Path::new("/Volumes/out")));
}
