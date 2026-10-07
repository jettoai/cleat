//! The paired Bluetooth devices, read from `system_profiler` (Swift `SystemProfilerPairings`,
//! `BluetoothHeadsets.swift:39-199`). Not `IOBluetoothDevice pairedDevices`: that call wedged
//! 0.3.0 for good in a process holding no Bluetooth privacy decision.

use std::cell::RefCell;
use std::time::{Duration, SystemTime};

use serde_json::Value;

use super::child::run_with_deadline;
use crate::model::BluetoothHeadset;

/// The paired devices. A trait so the engine and its tests can be given a list without a radio.
pub trait BluetoothInventory {
    fn paired_headsets(&self) -> Vec<BluetoothHeadset>;
}

/// How long a reconcile beat waits for the list. The limit is for the answer that never comes.
pub const TIMEOUT: Duration = Duration::from_secs(5);

/// How long an answer worth keeping is reused; tied to the request throttle on purpose.
pub const REUSE_WINDOW: Duration = Duration::from_secs(30);

pub type Reader = Box<dyn Fn() -> Option<Vec<BluetoothHeadset>>>;

pub struct SystemProfilerPairings {
    now: Box<dyn Fn() -> SystemTime>,
    read: Reader,
    answer: RefCell<Option<(Vec<BluetoothHeadset>, SystemTime)>>,
}

impl Default for SystemProfilerPairings {
    fn default() -> Self {
        Self::new(Box::new(SystemTime::now), Box::new(query))
    }
}

impl SystemProfilerPairings {
    /// The clock and the query are arguments so the reuse window can be tested without a radio.
    pub fn new(now: Box<dyn Fn() -> SystemTime>, read: Reader) -> Self {
        Self { now, read, answer: RefCell::new(None) }
    }

    /// The last answer while it is still worth reusing; a clock gone backwards makes it stale.
    fn reusable(&self, moment: SystemTime) -> Option<Vec<BluetoothHeadset>> {
        let answer = self.answer.borrow();
        let (headsets, asked) = answer.as_ref()?;
        let age = moment.duration_since(*asked).ok()?;
        (age < REUSE_WINDOW).then(|| headsets.clone())
    }
}

impl BluetoothInventory for SystemProfilerPairings {
    fn paired_headsets(&self) -> Vec<BluetoothHeadset> {
        let moment = (self.now)();
        if let Some(fresh) = self.reusable(moment) {
            return fresh;
        }
        // Neither a failed reading nor an empty one is an answer to keep.
        match (self.read)() {
            Some(headsets) if !headsets.is_empty() => {
                *self.answer.borrow_mut() = Some((headsets.clone(), moment));
                headsets
            }
            _ => vec![],
        }
    }
}

/// One reading of the pairing list, or None when the tool failed or did not answer in time.
pub fn query() -> Option<Vec<BluetoothHeadset>> {
    let data = run_with_deadline("/usr/sbin/system_profiler", &["SPBluetoothDataType", "-json"], TIMEOUT)?;
    Some(parse(&data))
}

/// One `system_profiler SPBluetoothDataType -json` document, as headsets. A device without an
/// address is skipped: the address is what a request is addressed to.
pub fn parse(data: &[u8]) -> Vec<BluetoothHeadset> {
    let Ok(root) = serde_json::from_slice::<Value>(data) else { return vec![] };
    let Some(report) = root.get("SPBluetoothDataType").and_then(|v| v.get(0)) else { return vec![] };
    let mut headsets = vec![];
    for (key, is_connected) in [("device_connected", true), ("device_not_connected", false)] {
        let Some(entries) = report.get(key).and_then(Value::as_array) else { continue };
        for entry in entries {
            // Each entry is a one-key object: the device name maps to its fields.
            let Some((name, fields)) = entry.as_object().and_then(|o| o.iter().next()) else { continue };
            let Some(address) = fields.get("device_address").and_then(Value::as_str) else { continue };
            headsets.push(BluetoothHeadset {
                name: name.clone(),
                address: BluetoothHeadset::canonical_address(address),
                is_connected,
            });
        }
    }
    headsets
}
