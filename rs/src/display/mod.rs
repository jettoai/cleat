//! The display side of the machine, as the engine sees it (Swift `Core/Display`). A trait so the
//! engine's bookkeeping can be tested without moving a real monitor.

mod coregraphics;

pub use coregraphics::CoreGraphicsDisplays;

/// `CGDisplayBounds`: the main display's top left is (0, 0) and y grows downwards.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// One online display as the rules see it.
#[derive(Debug, Clone, PartialEq)]
pub struct DisplaySnapshot {
    /// This connection's id: it changes across a reconnect, the UUID does not.
    pub id: u32,
    /// `CGDisplayCreateUUIDFromDisplayID`, the key macOS itself files a saved arrangement under;
    /// `unknown-<id>` when macOS has none.
    pub uuid: String,
    /// `NSScreen.localizedName`. Two monitors of one model share it.
    pub name: String,
    pub is_builtin: bool,
    pub is_main: bool,
    pub bounds: Rect,
}

impl DisplaySnapshot {
    /// The top left, rounded: only the origin counts, a resolution change is not a move.
    pub fn origin(&self) -> [i32; 2] {
        [self.bounds.x.round() as i32, self.bounds.y.round() as i32]
    }
}

/// Where one display's top left goes, in the global coordinate space of `CGDisplayBounds`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayOrigin {
    pub id: u32,
    pub x: i32,
    pub y: i32,
}

pub trait DisplaySystem {
    /// Empty when the system failed to answer, never a Mac with no screen.
    fn snapshot(&self) -> Vec<DisplaySnapshot>;
    /// Moves several displays in one configuration transaction, permanently: all of them or none.
    /// Written one at a time, a half-moved set overlaps or leaves a gap and is refused. False when
    /// macOS refused or the configuration could not be started.
    fn set_origins(&self, origins: &[DisplayOrigin]) -> bool;
    /// From now on, each finished reconfiguration sends `Event::DisplaysChanged` (possibly many
    /// for one plug-in, and for changes Cleat made itself).
    fn start_observing(&self);
}
