//! The private Smart Routing path (Swift `SmartRoutingClient`, `SmartRoutingClient.swift:83-226`):
//! `BTAudioRoutingRequest` in AudioAccessoryServices, asked with a score that says "this Mac is
//! playing". There is no promise it exists in the next macOS, so every piece is looked up first
//! and a missing one turns the rule off rather than crashing the daemon.
//!
//! Setters are called directly rather than through KVC: their type encodings were read off the
//! class on macOS 26 (`setAudioScore:` `v20@0:8i16`, `setFlags:` `v20@0:8I16`, `activate`
//! `v16@0:8`), and every selector is checked with `instancesRespondToSelector:` before use, so no
//! call can raise "unrecognized selector".

use std::collections::HashMap;
use std::ffi::CString;
use std::sync::Arc;

use block2::RcBlock;
use dispatch2::{DispatchQueue, DispatchRetained};
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, Bool, Sel};
use objc2::msg_send;
use objc2_foundation::NSString;

use super::response::RouteResponse;

/// Asking for a headset back. A trait so the engine can be tested without the private framework.
pub trait RouteRequesting {
    /// False when the class is gone, renamed, or lacks a property Cleat sets.
    fn is_available(&self) -> bool;
    /// Sends one request. The answer, if one arrives, comes back through the client's delivery.
    fn request(&mut self, name: &str, address: &str, score: i32, reason: &str);
    /// The answer for `address` arrived: the request object can go.
    fn finish(&mut self, _address: &str) {}
}

/// Where answers go: name, address, response. Called on the routing queue.
pub type Deliver = Arc<dyn Fn(String, String, RouteResponse) + Send + Sync>;

const FRAMEWORK: &str = "/System/Library/PrivateFrameworks/AudioAccessoryServices.framework/AudioAccessoryServices";
/// Flags value 1 is Hijack: "take this device from whoever has it".
pub const HIJACK_FLAG: u32 = 1;
const REQUIRED_KEYS: [&str; 7] =
    ["deviceAddress", "appBundleID", "audioScore", "flags", "reason", "dispatchQueue", "responseHandler"];

pub struct SmartRoutingClient {
    class: Option<&'static AnyClass>,
    bundle_id: String,
    deliver: Deliver,
    queue: DispatchRetained<DispatchQueue>,
    /// The requests waiting for an answer, one per address. A request owns the XPC connection its
    /// answer comes back on, so it is held here, not by its own handler (that would be a cycle).
    pending: HashMap<String, Retained<AnyObject>>,
}

/// `deviceAddress` to `setDeviceAddress:`.
pub fn setter_name(key: &str) -> String {
    let mut chars = key.chars();
    let first = chars.next().map(|c| c.to_uppercase().to_string()).unwrap_or_default();
    format!("set{first}{}:", chars.as_str())
}

fn sel(name: &str) -> Option<Sel> {
    Some(Sel::register(&CString::new(name).ok()?))
}

fn responds(class: &AnyClass, selector: &str) -> bool {
    let Some(s) = sel(selector) else { return false };
    // SAFETY: `instancesRespondToSelector:` is an NSObject class method taking a SEL.
    let r: Bool = unsafe { msg_send![class, instancesRespondToSelector: s] };
    r.as_bool()
}

fn resolve() -> Option<&'static AnyClass> {
    let path = CString::new(FRAMEWORK).ok()?;
    // SAFETY: dlopen of a system framework path; the handle is intentionally never closed.
    if unsafe { libc::dlopen(path.as_ptr(), libc::RTLD_NOW) }.is_null() {
        return None;
    }
    let class = AnyClass::get(c"BTAudioRoutingRequest")?;
    let complete = responds(class, "activate") && REQUIRED_KEYS.iter().all(|k| responds(class, &setter_name(k)));
    complete.then_some(class)
}

impl SmartRoutingClient {
    /// `bundle_id` is what the request names as the asking app.
    pub fn new(bundle_id: String, deliver: Deliver) -> Self {
        Self {
            class: resolve(),
            bundle_id,
            deliver,
            queue: DispatchQueue::new("ai.jetto.cleat.routing", None),
            pending: HashMap::new(),
        }
    }
}

impl RouteRequesting for SmartRoutingClient {
    fn is_available(&self) -> bool {
        self.class.is_some()
    }

    fn request(&mut self, name: &str, address: &str, score: i32, reason: &str) {
        let Some(class) = self.class else { return };
        // SAFETY: every selector below was checked in `resolve`, with the argument types its
        // encoding declares; `new` returns +1, owned by `Retained`.
        unsafe {
            let Some(request): Option<Retained<AnyObject>> = msg_send![class, new] else { return };
            let _: () = msg_send![&*request, setDeviceAddress: &*NSString::from_str(address)];
            let _: () = msg_send![&*request, setAppBundleID: &*NSString::from_str(&self.bundle_id)];
            let _: () = msg_send![&*request, setAudioScore: score];
            let _: () = msg_send![&*request, setFlags: HIJACK_FLAG];
            let _: () = msg_send![&*request, setReason: &*NSString::from_str(reason)];
            let queue: *const AnyObject = (&*self.queue as *const DispatchQueue).cast();
            let _: () = msg_send![&*request, setDispatchQueue: queue];
            let (deliver, name, addr) = (self.deliver.clone(), name.to_string(), address.to_string());
            // The handler captures no reference to the request; `pending` keeps it alive.
            let handler = RcBlock::new(move |raw: *mut AnyObject| {
                deliver(name.clone(), addr.clone(), read(raw));
            });
            let _: () = msg_send![&*request, setResponseHandler: &*handler];
            let _: () = msg_send![&*request, activate];
            self.pending.insert(address.to_string(), request);
        }
    }

    fn finish(&mut self, address: &str) {
        self.pending.remove(address);
    }
}

/// SAFETY: `obj` is a live object.
unsafe fn is_kind(obj: &AnyObject, class: &std::ffi::CStr) -> bool {
    let Some(c) = AnyClass::get(class) else { return false };
    let r: Bool = msg_send![obj, isKindOfClass: c];
    r.as_bool()
}

/// SAFETY: `obj` is a live object.
unsafe fn value(obj: &AnyObject, key: &str) -> Option<Retained<AnyObject>> {
    msg_send![obj, valueForKey: &*NSString::from_str(key)]
}

/// SAFETY: `obj` is an NSString.
unsafe fn text(obj: &AnyObject) -> String {
    (*(obj as *const AnyObject as *const NSString)).to_string()
}

/// Reads the three fields off a `BTAudioRoutingResponse`; every one is optional there.
fn read(raw: *mut AnyObject) -> RouteResponse {
    // SAFETY: the response object is alive for the duration of the handler call; every value is
    // class-checked before it is used as that class.
    unsafe {
        let Some(response) = raw.as_ref() else {
            return RouteResponse { error: Some("no response object".into()), ..Default::default() };
        };
        let action = value(response, "action").filter(|v| is_kind(v, c"NSNumber")).map(|v| {
            let n: isize = msg_send![&*v, integerValue];
            n as i64
        });
        let reason = value(response, "reason").filter(|v| is_kind(v, c"NSString")).map(|v| text(&v));
        let error = value(response, "error").and_then(|v| {
            if is_kind(&v, c"NSNumber") {
                let n: isize = msg_send![&*v, integerValue];
                (n != 0).then(|| format!("code {n}"))
            } else if is_kind(&v, c"NSError") {
                let d: Retained<NSString> = msg_send![&*v, localizedDescription];
                Some(d.to_string())
            } else if is_kind(&v, c"NSString") {
                Some(text(&v)).filter(|s| !s.is_empty())
            } else {
                None
            }
        });
        RouteResponse { action, reason, error }
    }
}
