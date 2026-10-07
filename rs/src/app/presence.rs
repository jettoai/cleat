//! Reads the facts `model::presence::judge` decides on. Nothing here needs a permission: the HID
//! idle counter is what `ioreg` shows any user, the assertions are what `pmset -g assertions`
//! shows any user, and the front app is what `lsappinfo front` shows any user.

use std::ffi::{c_char, c_void, CStr, CString};
use std::time::Duration;

use crate::model::presence::{DisplayAssertion, PresenceFacts, DISPLAY_ASSERTION_TYPES};
use crate::reclaim::child::run_with_deadline;

type CFTypeRef = *const c_void;

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(cf: CFTypeRef);
    fn CFGetTypeID(cf: CFTypeRef) -> usize;
    fn CFStringGetTypeID() -> usize;
    fn CFNumberGetTypeID() -> usize;
    fn CFArrayGetTypeID() -> usize;
    fn CFStringCreateWithCString(alloc: CFTypeRef, s: *const c_char, encoding: u32) -> CFTypeRef;
    fn CFStringGetCString(s: CFTypeRef, buf: *mut c_char, size: isize, encoding: u32) -> u8;
    fn CFNumberGetValue(n: CFTypeRef, kind: isize, out: *mut c_void) -> u8;
    fn CFDictionaryGetCount(d: CFTypeRef) -> isize;
    fn CFDictionaryGetKeysAndValues(d: CFTypeRef, keys: *mut CFTypeRef, values: *mut CFTypeRef);
    fn CFDictionaryGetValue(d: CFTypeRef, key: CFTypeRef) -> CFTypeRef;
    fn CFArrayGetCount(a: CFTypeRef) -> isize;
    fn CFArrayGetValueAtIndex(a: CFTypeRef, i: isize) -> CFTypeRef;
}

#[link(name = "IOKit", kind = "framework")]
extern "C" {
    fn IOPMCopyAssertionsByProcess(out: *mut CFTypeRef) -> i32;
    fn IOServiceMatching(name: *const c_char) -> CFTypeRef;
    fn IOServiceGetMatchingService(main_port: u32, matching: CFTypeRef) -> u32;
    fn IORegistryEntryCreateCFProperty(entry: u32, key: CFTypeRef, alloc: CFTypeRef, options: u32) -> CFTypeRef;
    fn IOObjectRelease(object: u32) -> i32;
}

const UTF8: u32 = 0x0800_0100;
const CF_NUMBER_SINT64: isize = 4;

/// Owns one CF reference.
struct Cf(CFTypeRef);

impl Drop for Cf {
    fn drop(&mut self) {
        // SAFETY: created or copied by a CF/IOKit call that returned +1, never null here.
        unsafe { CFRelease(self.0) }
    }
}

fn cf_str(s: &str) -> Option<Cf> {
    let c = CString::new(s).ok()?;
    // SAFETY: valid NUL-terminated buffer; null allocator is the default.
    let r = unsafe { CFStringCreateWithCString(std::ptr::null(), c.as_ptr(), UTF8) };
    (!r.is_null()).then_some(Cf(r))
}

/// SAFETY: `v` is a valid CF object or null.
unsafe fn string(v: CFTypeRef) -> Option<String> {
    if v.is_null() || CFGetTypeID(v) != CFStringGetTypeID() {
        return None;
    }
    let mut buf = [0 as c_char; 512];
    (CFStringGetCString(v, buf.as_mut_ptr(), buf.len() as isize, UTF8) != 0)
        .then(|| CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned())
}

/// SAFETY: `v` is a valid CF object or null.
unsafe fn int(v: CFTypeRef) -> Option<i64> {
    if v.is_null() || CFGetTypeID(v) != CFNumberGetTypeID() {
        return None;
    }
    let mut out: i64 = 0;
    (CFNumberGetValue(v, CF_NUMBER_SINT64, (&mut out as *mut i64).cast()) != 0).then_some(out)
}

/// Seconds since the last keyboard, mouse or trackpad event.
pub fn input_idle_seconds() -> Option<f64> {
    let key = cf_str("HIDIdleTime")?;
    // SAFETY: IOServiceMatching's dictionary is consumed by IOServiceGetMatchingService.
    let service = unsafe { IOServiceGetMatchingService(0, IOServiceMatching(c"IOHIDSystem".as_ptr())) };
    if service == 0 {
        return None;
    }
    // SAFETY: valid service and key; the returned property is +1 and owned by `Cf`.
    let value = unsafe { IORegistryEntryCreateCFProperty(service, key.0, std::ptr::null(), 0) };
    // SAFETY: service came from IOServiceGetMatchingService.
    unsafe { IOObjectRelease(service) };
    if value.is_null() {
        return None;
    }
    let value = Cf(value);
    // SAFETY: valid CF object.
    unsafe { int(value.0) }.map(|ns| ns as f64 / 1_000_000_000.0)
}

/// Every display-sleep assertion held right now (`IOPMCopyAssertionsByProcess`).
pub fn display_assertions() -> Option<Vec<DisplayAssertion>> {
    let mut dict: CFTypeRef = std::ptr::null();
    // SAFETY: out-pointer to a local.
    if unsafe { IOPMCopyAssertionsByProcess(&mut dict) } != 0 || dict.is_null() {
        return None;
    }
    let dict = Cf(dict);
    let (type_key, name_key, process_key) = (cf_str("AssertType")?, cf_str("AssertName")?, cf_str("Process Name")?);
    let mut found = vec![];
    // SAFETY: `dict` is a CFDictionary of CFNumber pid -> CFArray of CFDictionary, as documented
    // in IOPMLib.h:694-710; every value is type-checked before use.
    unsafe {
        let n = CFDictionaryGetCount(dict.0).max(0) as usize;
        let mut keys = vec![std::ptr::null(); n];
        let mut values = vec![std::ptr::null(); n];
        CFDictionaryGetKeysAndValues(dict.0, keys.as_mut_ptr(), values.as_mut_ptr());
        for (k, list) in keys.into_iter().zip(values) {
            let Some(pid) = int(k) else { continue };
            if list.is_null() || CFGetTypeID(list) != CFArrayGetTypeID() {
                continue;
            }
            for i in 0..CFArrayGetCount(list) {
                let a = CFArrayGetValueAtIndex(list, i);
                let Some(kind) = string(CFDictionaryGetValue(a, type_key.0)) else { continue };
                if !DISPLAY_ASSERTION_TYPES.contains(&kind.as_str()) {
                    continue;
                }
                found.push(DisplayAssertion {
                    pid: pid as i32,
                    process: string(CFDictionaryGetValue(a, process_key.0)).unwrap_or_else(|| format!("pid {pid}")),
                    name: string(CFDictionaryGetValue(a, name_key.0)).unwrap_or_default(),
                });
            }
        }
    }
    found.sort_by(|a, b| (a.pid, &a.name).cmp(&(b.pid, &b.name)));
    Some(found)
}

/// The frontmost app's pid, from `lsappinfo` (an ASN, then its pid). Two short subprocesses,
/// paid only when a video assertion is held and the keyboard is idle.
pub fn front_pid() -> Option<i32> {
    let limit = Duration::from_secs(1);
    let asn = String::from_utf8(run_with_deadline("/usr/bin/lsappinfo", &["front"], limit)?).ok()?;
    let info = run_with_deadline("/usr/bin/lsappinfo", &["info", "-only", "pid", asn.trim()], limit)?;
    parse_pid(&String::from_utf8_lossy(&info))
}

/// `pid = 21981` or `"pid"=21981`, whichever form this macOS prints.
pub fn parse_pid(text: &str) -> Option<i32> {
    for marker in ["pid = ", "\"pid\"="] {
        if let Some(at) = text.find(marker) {
            let digits: String = text[at + marker.len()..].chars().take_while(char::is_ascii_digit).collect();
            if let Ok(pid) = digits.parse() {
                return Some(pid);
            }
        }
    }
    None
}

/// The assertions, and the front app only when one of them is a video.
pub fn read_assertions_and_front() -> PresenceFacts {
    let display_assertions = display_assertions();
    let wants_front = display_assertions.as_ref().is_some_and(|all| all.iter().any(DisplayAssertion::is_video));
    PresenceFacts { input_idle: None, display_assertions, front_pid: if wants_front { front_pid() } else { None } }
}
