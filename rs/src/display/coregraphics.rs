//! The real display system: public CoreGraphics only, no SkyLight (Swift `CoreGraphicsDisplays`).
//! There is no CoreGraphics crate in the tree, so the dozen calls are declared here.

use std::collections::HashMap;
use std::ffi::c_void;
use std::ptr::{self, NonNull};
use std::sync::mpsc::Sender;

use dispatch2::DispatchQueue;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::MainThreadMarker;
use objc2_app_kit::NSScreen;
use objc2_core_foundation::{CFRetained, CFString, CFUUID};
use objc2_foundation::{ns_string, NSNumber};

use super::{DisplayOrigin, DisplaySnapshot, DisplaySystem, Rect};
use crate::engine::Event;

type DisplayId = u32;
type CGError = i32;
type ConfigRef = *mut c_void;
type ReconfigurationCallback = extern "C" fn(display: DisplayId, flags: u32, user: *mut c_void);

#[repr(C)]
struct CGPoint {
    x: f64,
    y: f64,
}
#[repr(C)]
struct CGSize {
    width: f64,
    height: f64,
}
#[repr(C)]
struct CGRect {
    origin: CGPoint,
    size: CGSize,
}

const SUCCESS: CGError = 0;
/// `kCGConfigurePermanently`.
const PERMANENTLY: u32 = 2;
/// `kCGDisplayBeginConfigurationFlag`.
const BEGIN_CONFIGURATION: u32 = 1;
const NULL_DISPLAY: DisplayId = 0;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn CGGetOnlineDisplayList(max: u32, displays: *mut DisplayId, count: *mut u32) -> CGError;
    fn CGDisplayBounds(display: DisplayId) -> CGRect;
    fn CGDisplayIsBuiltin(display: DisplayId) -> u32;
    fn CGDisplayIsMain(display: DisplayId) -> u32;
    fn CGDisplayMirrorsDisplay(display: DisplayId) -> DisplayId;
    /// A +1 `CFUUIDRef`, or null.
    fn CGDisplayCreateUUIDFromDisplayID(display: DisplayId) -> *const CFUUID;
    fn CGBeginDisplayConfiguration(config: *mut ConfigRef) -> CGError;
    fn CGConfigureDisplayOrigin(config: ConfigRef, display: DisplayId, x: i32, y: i32) -> CGError;
    fn CGCompleteDisplayConfiguration(config: ConfigRef, option: u32) -> CGError;
    fn CGCancelDisplayConfiguration(config: ConfigRef) -> CGError;
    fn CGDisplayRegisterReconfigurationCallback(callback: ReconfigurationCallback, user: *mut c_void) -> CGError;
}

pub struct CoreGraphicsDisplays {
    events: Sender<Event>,
}

impl CoreGraphicsDisplays {
    pub fn new(events: Sender<Event>) -> Self {
        Self { events }
    }
}

impl DisplaySystem for CoreGraphicsDisplays {
    fn snapshot(&self) -> Vec<DisplaySnapshot> {
        let mut count = 0u32;
        // SAFETY: a null list with max 0 only asks for the count.
        if unsafe { CGGetOnlineDisplayList(0, ptr::null_mut(), &mut count) } != SUCCESS || count == 0 {
            return vec![];
        }
        let mut ids = vec![NULL_DISPLAY; count as usize];
        // SAFETY: `ids` holds `count` ids.
        if unsafe { CGGetOnlineDisplayList(count, ids.as_mut_ptr(), &mut count) } != SUCCESS {
            return vec![];
        }
        ids.truncate(count as usize);
        let names = screen_names();
        ids.into_iter()
            .map(|id| {
                // SAFETY: plain queries on an id the system just listed.
                let (bounds, builtin, main) = unsafe { (CGDisplayBounds(id), CGDisplayIsBuiltin(id), CGDisplayIsMain(id)) };
                DisplaySnapshot {
                    id,
                    uuid: uuid_of(id),
                    name: names.get(&id).cloned().unwrap_or_else(|| format!("Display {id}")),
                    is_builtin: builtin != 0,
                    is_main: main != 0,
                    bounds: Rect {
                        x: bounds.origin.x,
                        y: bounds.origin.y,
                        width: bounds.size.width,
                        height: bounds.size.height,
                    },
                }
            })
            .collect()
    }

    fn set_origins(&self, origins: &[DisplayOrigin]) -> bool {
        // A mirrored display has no origin of its own to set.
        // SAFETY: a plain query.
        let settable: Vec<&DisplayOrigin> =
            origins.iter().filter(|o| unsafe { CGDisplayMirrorsDisplay(o.id) } == NULL_DISPLAY).collect();
        if settable.is_empty() {
            return true;
        }
        let mut config: ConfigRef = ptr::null_mut();
        // SAFETY: `config` is the out-pointer; every path below completes or cancels it.
        unsafe {
            if CGBeginDisplayConfiguration(&mut config) != SUCCESS || config.is_null() {
                return false;
            }
            for o in settable {
                if CGConfigureDisplayOrigin(config, o.id, o.x, o.y) != SUCCESS {
                    CGCancelDisplayConfiguration(config);
                    return false;
                }
            }
            CGCompleteDisplayConfiguration(config, PERMANENTLY) == SUCCESS
        }
    }

    fn start_observing(&self) {
        // The registration lives as long as the daemon does: the box is never freed. Registered
        // from the main thread, whose run loop is what delivers the callback.
        let user = Box::into_raw(Box::new(self.events.clone())) as usize;
        DispatchQueue::main().exec_async(move || {
            // SAFETY: `user` is the leaked `Sender<Event>` above.
            unsafe { CGDisplayRegisterReconfigurationCallback(reconfigured, user as *mut c_void) };
        });
    }
}

/// Every reconfiguration is announced twice, before and after. Only the second half describes a
/// layout that exists.
extern "C" fn reconfigured(_display: DisplayId, flags: u32, user: *mut c_void) {
    if flags & BEGIN_CONFIGURATION != 0 || user.is_null() {
        return;
    }
    // SAFETY: `user` is the `Sender<Event>` leaked by `start_observing`.
    let events = unsafe { &*(user as *const Sender<Event>) };
    let _ = events.send(Event::DisplaysChanged);
}

fn uuid_of(id: DisplayId) -> String {
    // SAFETY: a +1 reference or null.
    let raw = unsafe { CGDisplayCreateUUIDFromDisplayID(id) };
    let unknown = || format!("unknown-{id}");
    let Some(raw) = NonNull::new(raw.cast_mut()) else { return unknown() };
    // SAFETY: the Create rule hands over one retain.
    let uuid: CFRetained<CFUUID> = unsafe { CFRetained::from_raw(raw) };
    CFUUID::new_string(None, Some(&uuid)).map_or_else(unknown, |s: CFRetained<CFString>| s.to_string())
}

/// `NSScreen` belongs to the main thread, and this runs on the engine thread.
fn screen_names() -> HashMap<DisplayId, String> {
    let mut names = HashMap::new();
    let read = |names: &mut HashMap<DisplayId, String>, mtm: MainThreadMarker| {
        for screen in NSScreen::screens(mtm) {
            let description = screen.deviceDescription();
            let number: Option<Retained<AnyObject>> = description.objectForKey(ns_string!("NSScreenNumber"));
            if let Some(number) = number.and_then(|n| n.downcast::<NSNumber>().ok()) {
                names.insert(number.unsignedIntValue(), screen.localizedName().to_string());
            }
        }
    };
    match MainThreadMarker::new() {
        Some(mtm) => read(&mut names, mtm),
        None => DispatchQueue::main().exec_sync(|| {
            if let Some(mtm) = MainThreadMarker::new() {
                read(&mut names, mtm);
            }
        }),
    }
    names
}
