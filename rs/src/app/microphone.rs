//! Microphone TCC, the one permission Cleat needs and only for silence detection (Swift
//! `PermissionManager.swift`). Two class methods on AVCaptureDevice, called through objc2 rather
//! than a whole AVFoundation binding.

use std::sync::mpsc::Sender;

use block2::RcBlock;
use objc2::runtime::{AnyClass, Bool};
use objc2::msg_send;
use objc2_foundation::NSString;

use crate::engine::Event;
use crate::model::MicrophonePermission;

#[link(name = "AVFoundation", kind = "framework")]
extern "C" {
    static AVMediaTypeAudio: &'static NSString;
}

fn capture_device() -> Option<&'static AnyClass> {
    AnyClass::get(c"AVCaptureDevice")
}

/// `AVAuthorizationStatus` values, AVCaptureDevice.h:2148-2151.
pub fn permission_for(status: isize) -> MicrophonePermission {
    match status {
        0 => MicrophonePermission::Pending,
        1 => MicrophonePermission::Denied("restricted".into()),
        2 => MicrophonePermission::Denied("denied".into()),
        3 => MicrophonePermission::Granted,
        _ => MicrophonePermission::Denied("unknown".into()),
    }
}

/// What TCC says right now, without asking anybody anything.
pub fn current() -> MicrophonePermission {
    let Some(class) = capture_device() else { return permission_for(-1) };
    // SAFETY: a documented class method taking an AVMediaType and returning NSInteger.
    let status: isize = unsafe { msg_send![class, authorizationStatusForMediaType: AVMediaTypeAudio] };
    permission_for(status)
}

/// Shows the dialog if it has never been answered, then sends the state it left behind. The
/// answer is read back with `current()` rather than trusted from the callback's Bool, so
/// "restricted" has the same single source.
pub fn request(tx: Sender<Event>) {
    if current() != MicrophonePermission::Pending {
        let _ = tx.send(Event::Microphone(current()));
        return;
    }
    let Some(class) = capture_device() else { return };
    let handler = RcBlock::new(move |_granted: Bool| {
        let _ = tx.send(Event::Microphone(current()));
    });
    // SAFETY: documented class method; the block is copied by the callee.
    let _: () = unsafe {
        msg_send![class, requestAccessForMediaType: AVMediaTypeAudio, completionHandler: &*handler]
    };
}
