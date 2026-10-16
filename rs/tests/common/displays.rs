//! The display fixtures, word for word from `Fixture.Displays` in CleatTests/Fixtures.swift, and the
//! display system as a value (Swift `FakeDisplaySystem`).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use cleat_rs::display::{DisplayOrigin, DisplaySnapshot, DisplaySystem, Rect};

pub fn screen(
    id: u32,
    name: &str,
    uuid: &str,
    (x, y, width, height): (i32, i32, i32, i32),
    builtin: bool,
    main: bool,
) -> DisplaySnapshot {
    DisplaySnapshot {
        id,
        uuid: uuid.into(),
        name: name.into(),
        is_builtin: builtin,
        is_main: main,
        bounds: Rect { x: f64::from(x), y: f64::from(y), width: f64::from(width), height: f64::from(height) },
    }
}

pub const DELL_A_UUID: &str = "AAAAAAAA-0000-0000-0000-000000000001";
pub const XV_A_UUID: &str = "CCCCCCCC-0000-0000-0000-000000000003";
pub const LAPTOP_UUID: &str = "37D8832A-2D66-02CA-B9F7-8F30A301B230";

pub fn dell_a() -> DisplaySnapshot {
    screen(1, "DELL U3223QE", DELL_A_UUID, (0, 0, 2560, 1440), false, true)
}
pub fn dell_b() -> DisplaySnapshot {
    screen(2, "DELL U3223QE", "BBBBBBBB-0000-0000-0000-000000000002", (2560, 0, 2560, 1440), false, false)
}
pub fn xv_a() -> DisplaySnapshot {
    screen(3, "XV272U", XV_A_UUID, (0, -1440, 2560, 1440), false, false)
}
pub fn xv_b() -> DisplaySnapshot {
    screen(4, "XV272U", "DDDDDDDD-0000-0000-0000-000000000004", (2560, -1440, 2560, 1440), false, false)
}
/// Union of the four is x 0...5120, y -1440...1440, so centred below it is x 1696, y 1440.
pub fn laptop(x: i32, y: i32) -> DisplaySnapshot {
    screen(5, "Built-in Retina Display", LAPTOP_UUID, (x, y, 1728, 1117), true, false)
}
pub fn four() -> Vec<DisplaySnapshot> {
    vec![dell_a(), dell_b(), xv_a(), xv_b()]
}
pub fn docked() -> Vec<DisplaySnapshot> {
    vec![dell_a(), dell_b(), xv_a(), xv_b(), laptop(5120, 0)]
}
pub fn placed() -> Vec<DisplaySnapshot> {
    vec![dell_a(), dell_b(), xv_a(), xv_b(), laptop(1696, 1440)]
}
/// What macOS did to the set after a reconnect: one monitor and the laptop on the right.
pub fn scrambled() -> Vec<DisplaySnapshot> {
    vec![dell_a(), dell_b(), screen(3, "XV272U", XV_A_UUID, (5120, -1440, 2560, 1440), false, false), xv_b(), laptop(5120, 0)]
}
/// The batch string `FakeDisplays::set_origins` records for a set written as it stands.
pub fn batch(displays: &[DisplaySnapshot]) -> String {
    let origins: Vec<DisplayOrigin> =
        displays.iter().map(|d| DisplayOrigin { id: d.id, x: d.origin()[0], y: d.origin()[1] }).collect();
    FakeDisplayState::batch(&origins)
}

/// The display system as a value. A write lands back in the list and (like macOS) is followed by a
/// reconfiguration event, which waits in `echoes` until the harness hands it to the engine.
#[derive(Default)]
pub struct FakeDisplayState {
    pub displays: RefCell<Vec<DisplaySnapshot>>,
    /// False is macOS taking the origin and putting the display back.
    pub honors_writes: Cell<bool>,
    pub write_succeeds: Cell<bool>,
    /// One entry per `set_origins` call: `displays:<id>:<x>,<y>;...`, ordered by id.
    pub batches: RefCell<Vec<String>>,
    pub snapshot_calls: Cell<usize>,
    pub observing: Cell<bool>,
    pub echoes: Cell<usize>,
}

impl FakeDisplayState {
    pub fn new(displays: Vec<DisplaySnapshot>) -> Rc<Self> {
        let s = Self { displays: RefCell::new(displays), ..Default::default() };
        s.honors_writes.set(true);
        s.write_succeeds.set(true);
        Rc::new(s)
    }

    pub fn batch(origins: &[DisplayOrigin]) -> String {
        let mut sorted = origins.to_vec();
        sorted.sort_by_key(|o| o.id);
        "displays:".to_string()
            + &sorted.iter().map(|o| format!("{}:{},{}", o.id, o.x, o.y)).collect::<Vec<_>>().join(";")
    }
}

pub struct FakeDisplays(pub Rc<FakeDisplayState>);

impl DisplaySystem for FakeDisplays {
    fn snapshot(&self) -> Vec<DisplaySnapshot> {
        self.0.snapshot_calls.set(self.0.snapshot_calls.get() + 1);
        self.0.displays.borrow().clone()
    }
    fn set_origins(&self, origins: &[DisplayOrigin]) -> bool {
        self.0.batches.borrow_mut().push(FakeDisplayState::batch(origins));
        if !self.0.write_succeeds.get() {
            return false;
        }
        if self.0.honors_writes.get() {
            for d in self.0.displays.borrow_mut().iter_mut() {
                if let Some(o) = origins.iter().find(|o| o.id == d.id) {
                    d.bounds = Rect { x: f64::from(o.x), y: f64::from(o.y), ..d.bounds };
                }
            }
        }
        self.0.echoes.set(self.0.echoes.get() + 1);
        true
    }
    fn start_observing(&self) {
        self.0.observing.set(true);
    }
}
