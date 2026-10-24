//! Routing, Bluetooth inventory and volume writer doubles (split from engine.rs for its line limit).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use cleat_rs::model::BluetoothHeadset;
use cleat_rs::reclaim::{BluetoothInventory, RouteRequesting, RouteResponse};

/// The routing daemon as one canned answer (Swift `FakeRouting`). `None` is a request that is
/// never answered. Answers queue in `delivered` until the harness hands them to the engine, which
/// is what the real client's GCD hop does.
pub struct FakeRoutingState {
    pub available: bool,
    pub answer: RefCell<Option<RouteResponse>>,
    pub addresses: RefCell<Vec<String>>,
    pub scores: RefCell<Vec<i32>>,
    pub reasons: RefCell<Vec<String>>,
    pub delivered: RefCell<Vec<(String, String, RouteResponse)>>,
}

pub struct FakeRouting(pub Rc<FakeRoutingState>);

impl RouteRequesting for FakeRouting {
    fn is_available(&self) -> bool {
        self.0.available
    }
    fn request(&mut self, name: &str, address: &str, score: i32, reason: &str) {
        self.0.addresses.borrow_mut().push(address.into());
        self.0.scores.borrow_mut().push(score);
        self.0.reasons.borrow_mut().push(reason.into());
        if let Some(a) = self.0.answer.borrow().clone() {
            self.0.delivered.borrow_mut().push((name.into(), address.into(), a));
        }
    }
}

pub struct FakePairingsState {
    pub headsets: RefCell<Vec<BluetoothHeadset>>,
    pub reads: Cell<usize>,
}

pub struct FakePairings(pub Rc<FakePairingsState>);

impl BluetoothInventory for FakePairings {
    fn paired_headsets(&self) -> Vec<BluetoothHeadset> {
        self.0.reads.set(self.0.reads.get() + 1);
        self.0.headsets.borrow().clone()
    }
}

/// The writer log as a script (Swift `FakeVolumeWriterSource`).
#[derive(Default)]
pub struct FakeWriterState {
    pub start_result: Cell<bool>,
    pub made: Cell<usize>,
    pub starts: Cell<usize>,
}

pub struct FakeWriterSource(pub Rc<FakeWriterState>);

impl cleat_rs::outvol::writer_log::VolumeWriterSource for FakeWriterSource {
    fn start(&mut self) -> bool {
        self.0.starts.set(self.0.starts.get() + 1);
        self.0.start_result.get()
    }
    fn stop(&mut self) {}
}
