//! Reading the pairing list (Swift `ReclaimTests`, the five `SystemProfilerPairings` tests).

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use cleat_rs::model::BluetoothHeadset;
use cleat_rs::reclaim::pairings::{parse, REUSE_WINDOW};
use cleat_rs::reclaim::{BluetoothInventory, SystemProfilerPairings};

const PAIRING_LIST: &str = r#"{
  "SPBluetoothDataType": [
    {
      "device_connected": [
        { "AirPods Max": { "device_address": "70:F9:4A:B6:0C:C9", "device_minorType": "Headphones" } }
      ],
      "device_not_connected": [
        { "Albert iPhone": { "device_address": "f8-c3-cc-85-69-4c" } },
        { "Nameless thing": { "device_minorType": "Keyboard" } }
      ]
    }
  ]
}"#;

fn air_pods() -> BluetoothHeadset {
    BluetoothHeadset { name: "AirPods Max".into(), address: "70:F9:4A:B6:0C:C9".into(), is_connected: true }
}

#[test]
fn pairing_list_parses_system_profiler_json() {
    let headsets = parse(PAIRING_LIST.as_bytes());
    assert_eq!(headsets.len(), 2);
    assert_eq!(headsets[0], air_pods());
    assert_eq!(
        headsets[1],
        BluetoothHeadset { name: "Albert iPhone".into(), address: "F8:C3:CC:85:69:4C".into(), is_connected: false }
    );
}

#[test]
fn unreadable_pairing_list_is_no_headsets() {
    assert!(parse(b"not json").is_empty());
    assert!(parse(b"").is_empty());
    assert!(parse(b"{}").is_empty());
    assert!(parse(br#"{"SPBluetoothDataType": []}"#).is_empty());
}

/// A clock and a counted reader whose answer the test sets.
struct Rig {
    now: Rc<Cell<SystemTime>>,
    answer: Rc<RefCell<Option<Vec<BluetoothHeadset>>>>,
    reads: Rc<Cell<usize>>,
    pairings: SystemProfilerPairings,
}

fn rig(answer: Option<Vec<BluetoothHeadset>>) -> Rig {
    let now = Rc::new(Cell::new(UNIX_EPOCH + Duration::from_secs(1_700_000_000)));
    let answer = Rc::new(RefCell::new(answer));
    let reads = Rc::new(Cell::new(0));
    let (n, a, r) = (now.clone(), answer.clone(), reads.clone());
    let pairings = SystemProfilerPairings::new(
        Box::new(move || n.get()),
        Box::new(move || {
            r.set(r.get() + 1);
            a.borrow().clone()
        }),
    );
    Rig { now, answer, reads, pairings }
}

#[test]
fn pairing_list_is_reused_for_as_long_as_a_request_is_throttled() {
    let r = rig(Some(vec![air_pods()]));
    assert_eq!(r.pairings.paired_headsets(), vec![air_pods()]);
    assert_eq!(r.pairings.paired_headsets(), vec![air_pods()]);
    assert_eq!(r.reads.get(), 1);
    r.now.set(r.now.get() + REUSE_WINDOW - Duration::from_secs(1));
    assert_eq!(r.pairings.paired_headsets(), vec![air_pods()]);
    assert_eq!(r.reads.get(), 1);
    r.now.set(r.now.get() + Duration::from_secs(2));
    assert_eq!(r.pairings.paired_headsets(), vec![air_pods()]);
    assert_eq!(r.reads.get(), 2);
}

#[test]
fn failed_pairing_list_is_not_remembered() {
    let r = rig(None);
    assert!(r.pairings.paired_headsets().is_empty());
    assert!(r.pairings.paired_headsets().is_empty());
    assert_eq!(r.reads.get(), 2);
    *r.answer.borrow_mut() = Some(vec![air_pods()]);
    assert_eq!(r.pairings.paired_headsets(), vec![air_pods()]);
    assert_eq!(r.pairings.paired_headsets(), vec![air_pods()]);
    assert_eq!(r.reads.get(), 3);
}

#[test]
fn empty_pairing_list_is_not_remembered() {
    let r = rig(Some(vec![]));
    assert!(r.pairings.paired_headsets().is_empty());
    assert!(r.pairings.paired_headsets().is_empty());
    assert_eq!(r.reads.get(), 2);
    *r.answer.borrow_mut() = Some(vec![air_pods()]);
    assert_eq!(r.pairings.paired_headsets(), vec![air_pods()]);
    assert_eq!(r.pairings.paired_headsets(), vec![air_pods()]);
    assert_eq!(r.reads.get(), 3);
}
