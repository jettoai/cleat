//! The "not used" rule (B-1287): what happens when a side's default is a blocked device. The cases
//! that used to live in the two pin rule suites moved here word for word (target ids and reasons
//! unchanged); the Mac Studio ones replay Albert's own machine (PM 0209).

mod common;

use std::collections::HashMap;

use cleat_rs::config::Config;
use cleat_rs::model::{
    Action, AudioDevice, DeviceSnapshot, Liveness, TRANSPORT_AGGREGATE, TRANSPORT_AIRPLAY, TRANSPORT_BUILT_IN,
    TRANSPORT_USB,
};
use cleat_rs::rules::blocked::{reconcile, Side, Verdict};
use common::*;

fn snapshot(devices: Vec<AudioDevice>, current: Option<&AudioDevice>, liveness: &[(&AudioDevice, Liveness)]) -> DeviceSnapshot {
    DeviceSnapshot {
        devices,
        default_input: current.map(|d| d.id),
        liveness: liveness.iter().map(|(d, l)| (d.uid.clone(), *l)).collect::<HashMap<_, _>>(),
        ..Default::default()
    }
}

fn outputs(devices: Vec<AudioDevice>, current: &AudioDevice) -> DeviceSnapshot {
    DeviceSnapshot { devices, default_output: Some(current.id), ..Default::default() }
}

fn input(snap: &DeviceSnapshot, config: &Config) -> Verdict {
    reconcile(Side::Input, snap, config)
}

fn output(snap: &DeviceSnapshot, config: &Config) -> Verdict {
    reconcile(Side::Output, snap, config)
}

fn to_in(id: u32, reason: &str) -> Verdict {
    Verdict::Evict(Action::SetDefaultInput(id, reason.into()))
}

fn to_out(id: u32, reason: &str) -> Verdict {
    Verdict::Evict(Action::SetDefaultOutput(id, reason.into()))
}

fn overlap(input: &[&str], blocked: &[&str]) -> Config {
    Config { input: s(input), blocked_input: s(blocked), ..Config::default() }
}

fn overlap_out(output: &[&str], blocked: &[&str]) -> Config {
    Config { output: s(output), blocked_output: s(blocked), ..Config::default() }
}

fn albert_extras() -> Vec<AudioDevice> {
    vec![air_pods(), zoom(), black_hole(), iphone_mic()]
}

fn maono_speakers() -> AudioDevice {
    AudioDevice::with_transport(51, "Maono\u{00A0}AI Microphone", "Maono-Output-UID", false, true, TRANSPORT_USB)
}

fn output_config() -> Config {
    Config { output: s(&["Studio Display Speakers", "MacBook Pro Speakers"]), ..Config::default() }
}

fn headset_only_config() -> Config {
    Config {
        output: s(&["AirPods Max"]),
        headphones_take_over: true,
        blocked_output: s(&["Maono AI Microphone"]),
        ..Config::default()
    }
}

// ---- Moved from input_pin_rule.rs ----

#[test]
fn blocked_device_is_replaced() {
    let snap = snapshot(vec![air_pods(), brio()], Some(&air_pods()), &[]);
    assert_eq!(input(&snap, &pinned_input()), to_in(brio().id, "AirPods Max -> Brio 100 (blocked)"));
}

#[test]
fn no_candidate_leaves_blocked_device_in_place() {
    let snap = snapshot(vec![air_pods()], Some(&air_pods()), &[]);
    assert_eq!(input(&snap, &pinned_input()), Verdict::Stuck(air_pods()));
}

#[test]
fn output_only_device_is_not_an_input_candidate() {
    let config =
        Config { input: s(&["Studio Display Speakers", "Brio 100"]), blocked_input: s(&["AirPods Max"]), ..Config::default() };
    let snap = snapshot(vec![display_speakers(), air_pods(), brio()], Some(&air_pods()), &[]);
    assert_eq!(input(&snap, &config), to_in(brio().id, "AirPods Max -> Brio 100 (blocked)"));
}

#[test]
fn blocked_listed_current_device_is_replaced_by_the_next_candidate() {
    let config = overlap(&["AirPods Max", "Brio 100"], &["AirPods Max"]);
    let snap = snapshot(vec![air_pods(), brio()], Some(&air_pods()), &[]);
    assert_eq!(input(&snap, &config), to_in(brio().id, "AirPods Max -> Brio 100 (blocked)"));
}

#[test]
fn blocked_listed_current_device_moves_to_a_silent_listed_device() {
    let config = overlap(&["Brio 100", "AirPods Max"], &["AirPods Max"]);
    let snap = snapshot(vec![air_pods(), brio()], Some(&air_pods()), &[(&brio(), Liveness::Silent)]);
    assert_eq!(input(&snap, &config), to_in(brio().id, "AirPods Max -> Brio 100 (blocked)"));
}

#[test]
fn blocked_listed_current_device_stays_without_another_candidate() {
    let config = overlap(&["AirPods Max", "Brio 100"], &["AirPods Max"]);
    let snap = snapshot(vec![air_pods()], Some(&air_pods()), &[]);
    assert_eq!(input(&snap, &config), Verdict::Stuck(air_pods()));
}

#[test]
fn uid_block_skips_to_the_next_candidate_from_outside_the_list() {
    let config = overlap(&["Brio 100", "Wireless microphone"], &[&brio().uid, "AirPods Max"]);
    let snap = snapshot(vec![brio(), wireless(), air_pods()], Some(&air_pods()), &[]);
    assert_eq!(input(&snap, &config), to_in(wireless().id, "AirPods Max -> Wireless microphone (blocked)"));
}

#[test]
fn albert_config_evicts_air_pods_to_the_built_in_microphone_before_the_silent_wireless_one() {
    let config = overlap(&["Wireless microphone", "Brio 100", "AirPods Max"], &["AirPods Max"]);
    let snap = snapshot(vec![wireless(), air_pods(), mac_mic()], Some(&air_pods()), &[(&wireless(), Liveness::Silent)]);
    assert_eq!(input(&snap, &config), to_in(mac_mic().id, "AirPods Max -> MacBook Pro Microphone (blocked)"));
}

#[test]
fn without_a_built_in_microphone_air_pods_go_to_the_silent_wireless_microphone() {
    let config = overlap(&["Wireless microphone", "Brio 100", "AirPods Max"], &["AirPods Max"]);
    let snap = snapshot(vec![wireless(), air_pods()], Some(&air_pods()), &[(&wireless(), Liveness::Silent)]);
    assert_eq!(input(&snap, &config), to_in(wireless().id, "AirPods Max -> Wireless microphone (blocked)"));
}

#[test]
fn a_measuring_listed_device_also_comes_after_the_built_in_microphone() {
    let snap = snapshot(vec![wireless(), air_pods(), mac_mic()], Some(&air_pods()), &[(&wireless(), Liveness::Measuring)]);
    assert_eq!(input(&snap, &pinned_input()), to_in(mac_mic().id, "AirPods Max -> MacBook Pro Microphone (blocked)"));
}

#[test]
fn blocked_current_input_falls_back_to_the_built_in_microphone_not_an_unlisted_one() {
    let snap = snapshot(vec![air_pods(), maono(), mac_mic()], Some(&air_pods()), &[]);
    assert_eq!(input(&snap, &pinned_input()), to_in(mac_mic().id, "AirPods Max -> MacBook Pro Microphone (blocked)"));
    let snap = snapshot(vec![air_pods(), maono()], Some(&air_pods()), &[]);
    assert_eq!(input(&snap, &pinned_input()), Verdict::Stuck(air_pods()));
}

#[test]
fn built_in_fallback_skips_blocked_and_output_only_devices() {
    let config = overlap(&["Wireless microphone"], &["AirPods Max", "Brio 100", "MacBook Pro Microphone"]);
    let other_mic = AudioDevice::with_transport(84, "Aaa Microphone", "Aaa-UID", true, false, TRANSPORT_BUILT_IN);
    let snap = snapshot(vec![air_pods(), brio(), mac_speakers(), mac_mic(), other_mic.clone()], Some(&air_pods()), &[]);
    assert_eq!(input(&snap, &config), to_in(other_mic.id, "AirPods Max -> Aaa Microphone (blocked)"));
    let snap = snapshot(vec![air_pods(), brio(), mac_speakers(), mac_mic()], Some(&air_pods()), &[]);
    assert_eq!(input(&snap, &config), Verdict::Stuck(air_pods()));
}

/// Albert's replay (a1): AirPods Max current, wireless microphone and Brio absent.
#[test]
fn blocked_input_falls_back_to_the_built_in_microphone_only() {
    let mut devices = albert_extras();
    devices.push(mac_mic());
    let snap = snapshot(devices, Some(&air_pods()), &[]);
    assert_eq!(input(&snap, &albert_input()), to_in(mac_mic().id, "AirPods Max -> MacBook Pro Microphone (blocked)"));
}

#[test]
fn blocked_input_without_a_built_in_microphone_stays() {
    let snap = snapshot(albert_extras(), Some(&air_pods()), &[]);
    assert_eq!(input(&snap, &albert_input()), Verdict::Stuck(air_pods()));
}

#[test]
fn only_virtual_inputs_are_never_a_fallback() {
    let snap = snapshot(vec![air_pods(), zoom()], Some(&air_pods()), &[]);
    assert_eq!(input(&snap, &pinned_input()), Verdict::Stuck(air_pods()));
}

/// Albert's replay (a2): the wireless microphone is there but silent; AirPods Max goes to the
/// built-in microphone, and back to the wireless one once it has signal (engine_eviction).
#[test]
fn albert_replay_silent_wireless_microphone_yields_to_the_built_in() {
    let mut devices = albert_extras();
    devices.extend([mac_mic(), wireless()]);
    let snap = snapshot(devices, Some(&air_pods()), &[(&wireless(), Liveness::Silent)]);
    assert_eq!(input(&snap, &albert_input()), to_in(mac_mic().id, "AirPods Max -> MacBook Pro Microphone (blocked)"));
}

#[test]
fn blocked_current_input_is_evicted_with_an_empty_list() {
    let config = Config { blocked_input: s(&["AirPods Max"]), ..Config::default() };
    let snap = snapshot(vec![air_pods(), mac_mic()], Some(&air_pods()), &[]);
    assert_eq!(input(&snap, &config), to_in(mac_mic().id, "AirPods Max -> MacBook Pro Microphone (blocked)"));
}

// ---- Moved from output_pin_rule.rs ----

#[test]
fn blocked_current_output_is_replaced_by_the_first_listed_device() {
    let mut config = output_config();
    config.blocked_output = s(&["Maono AI Microphone"]);
    let snap = outputs(vec![maono_speakers(), display_speakers(), mac_speakers()], &maono_speakers());
    assert_eq!(
        output(&snap, &config),
        to_out(display_speakers().id, "Maono\u{00A0}AI Microphone -> Studio Display Speakers (blocked)")
    );
}

/// Renamed from `blocked_current_output_without_a_built_in_escape_stays` (plan §5.3): what keeps
/// it in place is that nothing else is present, not the missing built-in output.
#[test]
fn blocked_current_output_with_nothing_else_present_stays() {
    let mut config = output_config();
    config.blocked_output = s(&["Maono AI Microphone"]);
    let snap = outputs(vec![maono_speakers()], &maono_speakers());
    assert_eq!(output(&snap, &config), Verdict::Stuck(maono_speakers()));
}

#[test]
fn blocked_bluetooth_device_is_still_replaced_with_takeover_on() {
    let mut config = output_config();
    config.headphones_take_over = true;
    config.blocked_output = s(&["AirPods Max"]);
    let snap = outputs(vec![air_pods(), display_speakers()], &air_pods());
    assert_eq!(output(&snap, &config), to_out(display_speakers().id, "AirPods Max -> Studio Display Speakers (blocked)"));
}

#[test]
fn blocked_current_output_is_evicted_to_the_built_in_output_when_every_listed_output_is_a_headset() {
    let snap = outputs(vec![maono_speakers(), air_pods(), display_speakers(), mac_speakers()], &maono_speakers());
    assert_eq!(
        output(&snap, &headset_only_config()),
        to_out(mac_speakers().id, "Maono\u{00A0}AI Microphone -> MacBook Pro Speakers (blocked)")
    );
}

#[test]
fn blocked_current_output_stays_when_the_only_escape_is_a_headset() {
    let snap = outputs(vec![maono_speakers(), air_pods()], &maono_speakers());
    assert_eq!(output(&snap, &headset_only_config()), Verdict::Stuck(maono_speakers()));
}

#[test]
fn blocked_listed_current_output_is_replaced_by_the_next_listed_device() {
    let config = overlap_out(&["Studio Display Speakers", "MacBook Pro Speakers"], &["Studio Display Speakers"]);
    let snap = outputs(vec![display_speakers(), mac_speakers()], &display_speakers());
    assert_eq!(output(&snap, &config), to_out(mac_speakers().id, "Studio Display Speakers -> MacBook Pro Speakers (blocked)"));
}

#[test]
fn fully_blocked_list_still_evicts_a_blocked_current_output() {
    let config = overlap_out(&["Studio Display Speakers"], &["Studio Display Speakers"]);
    let snap = outputs(vec![display_speakers(), mac_speakers()], &display_speakers());
    assert_eq!(output(&snap, &config), to_out(mac_speakers().id, "Studio Display Speakers -> MacBook Pro Speakers (blocked)"));
}

/// Albert's replay (b): only the wired headphones were listed, then marked "not used".
#[test]
fn blocked_current_output_is_evicted_with_an_empty_list() {
    let config = Config { blocked_output: s(&["外接耳機"]), ..Config::default() };
    let snap = outputs(vec![wired_headphones(), mac_speakers(), display_speakers()], &wired_headphones());
    assert_eq!(output(&snap, &config), to_out(mac_speakers().id, "外接耳機 -> MacBook Pro Speakers (blocked)"));
}

/// Renamed from `blocked_output_falls_back_to_the_built_in_output_only`, and its second half
/// rewritten: under the PM ruling (0209 #1) a physical USB output is an escape too, built-in first.
#[test]
fn blocked_output_falls_back_to_a_physical_output_built_in_first() {
    let config = overlap_out(&["Studio Display Speakers"], &["AirPods Max"]);
    let snap = outputs(vec![air_pods(), maono_speakers(), mac_speakers()], &air_pods());
    assert_eq!(output(&snap, &config), to_out(mac_speakers().id, "AirPods Max -> MacBook Pro Speakers (blocked)"));
    let snap = outputs(vec![air_pods(), maono_speakers()], &air_pods());
    assert_eq!(output(&snap, &config), to_out(maono_speakers().id, "AirPods Max -> Maono\u{00A0}AI Microphone (blocked)"));
}

// ---- Albert's Mac Studio (PM 0209 #1, #2) ----

/// MS-1 (PM ruling on B1): AirPods Max grabbed the microphone, the wireless one is there but
/// silent, Brio absent, no built-in microphone: the silent listed microphone, never the unlisted
/// Maono.
#[test]
fn mac_studio_air_pods_grabbed_the_mic_wireless_silent() {
    let snap = snapshot(mac_studio(false, true, true, false), Some(&air_pods()), &[(&wireless(), Liveness::Silent)]);
    assert_eq!(input(&snap, &albert_config()), to_in(wireless().id, "AirPods Max -> Wireless microphone (blocked)"));
}

/// MS-1b: the same with the wireless microphone unplugged: the Maono is physical but unlisted, so
/// nothing is left and AirPods Max is reported stuck.
#[test]
fn mac_studio_air_pods_grabbed_the_mic_wireless_absent_is_stuck() {
    let snap = snapshot(mac_studio(false, true, false, false), Some(&air_pods()), &[]);
    assert_eq!(input(&snap, &albert_config()), Verdict::Stuck(air_pods()));
}

/// MS-2
#[test]
fn mac_studio_air_pods_grabbed_the_mic_nothing_else_physical() {
    let devices = vec![mac_studio_speakers(), teams(), zoom(), air_pods(), continuity_iphone()];
    let snap = snapshot(devices, Some(&air_pods()), &[]);
    assert_eq!(input(&snap, &albert_config()), Verdict::Stuck(air_pods()));
}

fn jack_not_used() -> Config {
    let mut c = albert_config();
    c.output = vec![];
    c.blocked_output.push("外接耳機".into());
    c
}

/// MS-3: the PM's C-1 probe on the real machine.
#[test]
fn mac_studio_not_used_on_the_jack_goes_to_the_display() {
    let snap = outputs(mac_studio(true, false, false, false), &wired_headphones());
    assert_eq!(output(&snap, &jack_not_used()), to_out(dell().id, "外接耳機 -> DELL U3223QE (blocked)"));
}

/// MS-4
#[test]
fn mac_studio_not_used_on_the_jack_with_only_a_headset_is_stuck() {
    let snap = outputs(vec![mac_studio_speakers(), wired_headphones(), air_pods()], &wired_headphones());
    assert_eq!(output(&snap, &jack_not_used()), Verdict::Stuck(wired_headphones()));
}

/// MS-5
#[test]
fn mac_studio_speakers_blocked_jack_unplugged_goes_to_the_display() {
    let snap = outputs(mac_studio(false, false, false, false), &mac_studio_speakers());
    assert_eq!(output(&snap, &albert_config()), to_out(dell().id, "Mac Studio的揚聲器 -> DELL U3223QE (blocked)"));
}

fn aggregate() -> AudioDevice {
    AudioDevice::with_transport(97, "Aggregate Device", "Aggregate-UID", true, true, TRANSPORT_AGGREGATE)
}

fn apple_tv() -> AudioDevice {
    AudioDevice::with_transport(98, "Living Room", "AirPlay-UID", false, true, TRANSPORT_AIRPLAY)
}

/// MS-6: virtual, aggregate, Continuity, AirPlay and unknown-transport devices are never an escape.
#[test]
fn virtual_aggregate_continuity_airplay_and_unknown_are_never_an_escape() {
    let devices = vec![air_pods(), zoom(), black_hole(), iphone_mic(), continuity_iphone(), aggregate(), apple_tv()];
    let snap = snapshot(devices.clone(), Some(&air_pods()), &[]);
    assert_eq!(input(&snap, &pinned_input()), Verdict::Stuck(air_pods()));
    let config = Config { blocked_output: s(&["AirPods Max"]), ..Config::default() };
    assert_eq!(output(&outputs(devices, &air_pods()), &config), Verdict::Stuck(air_pods()));
}

/// MS-8
#[test]
fn built_in_comes_before_a_usb_device_by_name() {
    let config = Config { input: vec![], blocked_input: s(&["AirPods Max"]), ..Config::default() };
    let snap = snapshot(vec![air_pods(), mac_mic(), brio()], Some(&air_pods()), &[]);
    assert_eq!(input(&snap, &config), to_in(mac_mic().id, "AirPods Max -> MacBook Pro Microphone (blocked)"));
}

/// MS-8b: the output side's built-in-first order ("DELL U3223QE" sorts before "MacBook Pro Speakers").
#[test]
fn built_in_output_comes_before_a_display_by_name() {
    let config = Config { blocked_output: s(&["AirPods Max"]), ..Config::default() };
    let snap = outputs(vec![air_pods(), dell(), mac_speakers()], &air_pods());
    assert_eq!(output(&snap, &config), to_out(mac_speakers().id, "AirPods Max -> MacBook Pro Speakers (blocked)"));
}

/// MS-9
#[test]
fn a_listed_device_with_signal_comes_before_the_built_in() {
    let snap = snapshot(vec![air_pods(), wireless(), mac_mic()], Some(&air_pods()), &[(&wireless(), Liveness::Live)]);
    assert_eq!(input(&snap, &pinned_input()), to_in(wireless().id, "AirPods Max -> Wireless microphone (blocked)"));
}

/// PM ruling on B3: a "not used" device that is not physical is only never chosen. A meeting app
/// that switches the input to its own virtual device, or an iPhone over Continuity, is not
/// overruled (as in the Swift version), and that is not "stuck" either.
#[test]
fn a_blocked_virtual_or_continuity_default_is_left_to_the_app() {
    let snap = snapshot(mac_studio(true, false, true, false), Some(&teams()), &[(&wireless(), Liveness::Live)]);
    assert_eq!(input(&snap, &albert_config()), Verdict::Clear);
    let mut config = albert_config();
    config.blocked_input.push(continuity_iphone().name);
    let mut devices = mac_studio(true, false, true, false);
    devices.push(continuity_iphone());
    let snap = snapshot(devices, Some(&continuity_iphone()), &[(&wireless(), Liveness::Live)]);
    assert_eq!(input(&snap, &config), Verdict::Clear);
}

#[test]
fn an_unblocked_default_is_clear() {
    let snap = snapshot(vec![wireless(), brio()], Some(&brio()), &[]);
    assert_eq!(input(&snap, &pinned_input()), Verdict::Clear);
}
