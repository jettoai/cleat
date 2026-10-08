//! Property listeners: a C proc plus one leaked, never-freed context per kind. Every registration
//! of a kind shares that context, so removal uses the same (proc, data) pair and an in-flight
//! callback can never see freed memory. The proc only sends an event to the engine thread.

use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::mpsc::Sender;
use std::time::SystemTime;

use objc2_core_audio::{
    kAudioDevicePropertyDeviceIsRunningSomewhere, kAudioDevicePropertyVolumeScalar,
    kAudioHardwarePropertyDefaultInputDevice, kAudioHardwarePropertyDefaultOutputDevice,
    kAudioHardwarePropertyDevices, kAudioObjectPropertyElementMain, kAudioObjectPropertyScopeGlobal,
    kAudioObjectPropertyScopeInput, kAudioObjectPropertyScopeOutput, AudioObjectAddPropertyListener,
    AudioObjectID, AudioObjectPropertyAddress, AudioObjectPropertySelector, AudioObjectRemovePropertyListener,
};

use super::property::{address, SYSTEM_OBJECT};
use super::ListenTarget;
use crate::engine::Event;

/// `kAudioHardwareServiceDeviceProperty_VirtualMainBalance` ('vmbc', AudioHardwareService.h).
/// Served by the HAL itself: no AudioToolbox needed.
pub const VIRTUAL_MAIN_BALANCE: AudioObjectPropertySelector = u32::from_be_bytes(*b"vmbc");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ListenerKind {
    Devices,
    DefaultInput,
    DefaultOutput,
    Balance,
    Running,
    Volume,
    /// The default output's volume, output scope (the hold); `Volume` is the input side.
    OutputVolume,
}

impl ListenerKind {
    pub const ALL: [ListenerKind; 7] = [
        ListenerKind::Devices,
        ListenerKind::DefaultInput,
        ListenerKind::DefaultOutput,
        ListenerKind::Balance,
        ListenerKind::Running,
        ListenerKind::Volume,
        ListenerKind::OutputVolume,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ListenerKind::Devices => "devices",
            ListenerKind::DefaultInput => "defaultInput",
            ListenerKind::DefaultOutput => "defaultOutput",
            ListenerKind::Balance => "balance",
            ListenerKind::Running => "running",
            ListenerKind::Volume => "volume",
            ListenerKind::OutputVolume => "outputVolume",
        }
    }

    fn index(self) -> usize {
        self as usize
    }

    fn address(self, element: u32) -> AudioObjectPropertyAddress {
        let (sel, scope, el) = match self {
            ListenerKind::Devices => (kAudioHardwarePropertyDevices, kAudioObjectPropertyScopeGlobal, kAudioObjectPropertyElementMain),
            ListenerKind::DefaultInput => {
                (kAudioHardwarePropertyDefaultInputDevice, kAudioObjectPropertyScopeGlobal, kAudioObjectPropertyElementMain)
            }
            ListenerKind::DefaultOutput => {
                (kAudioHardwarePropertyDefaultOutputDevice, kAudioObjectPropertyScopeGlobal, kAudioObjectPropertyElementMain)
            }
            ListenerKind::Balance => (VIRTUAL_MAIN_BALANCE, kAudioObjectPropertyScopeOutput, kAudioObjectPropertyElementMain),
            ListenerKind::Running => {
                (kAudioDevicePropertyDeviceIsRunningSomewhere, kAudioObjectPropertyScopeGlobal, kAudioObjectPropertyElementMain)
            }
            ListenerKind::Volume => (kAudioDevicePropertyVolumeScalar, kAudioObjectPropertyScopeInput, element),
            ListenerKind::OutputVolume => (kAudioDevicePropertyVolumeScalar, kAudioObjectPropertyScopeOutput, element),
        };
        address(sel, scope, el)
    }
}

/// A registered listener, kept so it can be removed again.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ListenerToken {
    pub object: AudioObjectID,
    pub address: AudioObjectPropertyAddress,
    pub kind: ListenerKind,
}

struct ListenerCtx {
    kind: ListenerKind,
    tx: Sender<Event>,
}

pub(super) struct Contexts([&'static ListenerCtx; 7]);

impl Contexts {
    /// Leaks one small context per kind for the life of the process.
    pub(super) fn new(tx: &Sender<Event>) -> Self {
        Self(ListenerKind::ALL.map(|kind| &*Box::leak(Box::new(ListenerCtx { kind, tx: tx.clone() }))))
    }

    fn data(&self, kind: ListenerKind) -> *mut c_void {
        (self.0[kind.index()] as *const ListenerCtx).cast_mut().cast()
    }

    pub(super) fn add(&self, target: ListenTarget) -> Option<ListenerToken> {
        let (object, kind, element) = match target {
            ListenTarget::System(kind) => (SYSTEM_OBJECT, kind, kAudioObjectPropertyElementMain),
            ListenTarget::Device { device, kind, element } => (device, kind, element),
        };
        let mut addr = kind.address(element);
        // SAFETY: the context is 'static and the proc matches the HAL's signature.
        let status =
            unsafe { AudioObjectAddPropertyListener(object, NonNull::from(&mut addr), Some(on_property), self.data(kind)) };
        (status == 0).then_some(ListenerToken { object, address: addr, kind })
    }

    pub(super) fn remove(&self, token: ListenerToken) {
        let mut addr = token.address;
        // SAFETY: same (proc, data) pair the listener was added with.
        unsafe {
            AudioObjectRemovePropertyListener(token.object, NonNull::from(&mut addr), Some(on_property), self.data(token.kind));
        }
    }
}

unsafe extern "C-unwind" fn on_property(
    object: AudioObjectID,
    _count: u32,
    _addresses: NonNull<AudioObjectPropertyAddress>,
    data: *mut c_void,
) -> i32 {
    // SAFETY: `data` is one of the leaked 'static contexts.
    let ctx = unsafe { &*(data as *const ListenerCtx) };
    // The output volume hold must know which device spoke: a late reading from the previous
    // output is not the new output's (Swift `outputVolumeChanged(device)`).
    let event = if ctx.kind == ListenerKind::OutputVolume {
        Event::OutputVolumeChanged { device: object }
    } else {
        Event::Listener { kind: ctx.kind, received: SystemTime::now() }
    };
    let _ = ctx.tx.send(event);
    0
}
