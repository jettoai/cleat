//! The non-realtime half of a detector: device setup, the IOProc's lifetime and the flip source
//! (Swift `ZeroSignalDetector.swift:102-224`).

use std::cell::UnsafeCell;
use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;

use dispatch2::{DispatchObject, DispatchQueue, DispatchSource, _dispatch_source_type_data_add};
use objc2_core_audio::{
    AudioDeviceCreateIOProcID, AudioDeviceDestroyIOProcID, AudioDeviceIOProcID, AudioDeviceStart, AudioDeviceStop,
};

use super::ioproc::{io_proc, Shared};
use super::streak::ZeroStreak;
use super::LivenessDetecting;
use crate::audio::prepare_liveness_input;
use crate::engine::Event;
use crate::model::AudioDevice;

/// Lives in the flip source's context; freed by the source's cancel handler.
struct HandlerCtx {
    shared: Arc<Shared>,
    tx: Sender<Event>,
    uid: String,
    name: String,
}

extern "C" fn on_flip(ctx: *mut c_void) {
    // SAFETY: the context is a HandlerCtx until the cancel handler frees it, and the source runs
    // its handlers serially on its queue.
    let ctx = unsafe { &*ctx.cast::<HandlerCtx>() };
    let live = ctx.shared.silent.load(Ordering::Acquire) == 0;
    let _ = ctx.tx.send(Event::LivenessFlip { uid: ctx.uid.clone(), name: ctx.name.clone(), live });
}

extern "C" fn on_cancel(ctx: *mut c_void) {
    // SAFETY: set once from Box::into_raw in `flip_channel`; the cancel handler runs once.
    drop(unsafe { Box::from_raw(ctx.cast::<HandlerCtx>()) });
}

/// Builds the state an IOProc shares with the engine side, with its flip source already running.
/// Cancel `shared.source` to release it; the last owner frees it after the cancel handler.
pub fn flip_channel(
    threshold_frames: usize,
    bytes_per_sample: usize,
    uid: &str,
    name: &str,
    tx: Sender<Event>,
) -> Arc<Shared> {
    let queue = DispatchQueue::new("ai.jetto.cleat.liveness", None);
    // SAFETY: DATA_ADD takes handle 0 and mask 0.
    let source = unsafe {
        DispatchSource::new(std::ptr::addr_of!(_dispatch_source_type_data_add).cast_mut(), 0, 0, Some(&queue))
    };
    let shared = Arc::new(Shared {
        streak: UnsafeCell::new(ZeroStreak::new(threshold_frames)),
        bytes_per_sample,
        silent: AtomicI32::new(0),
        source,
    });
    let ctx = Box::into_raw(Box::new(HandlerCtx {
        shared: shared.clone(),
        tx,
        uid: uid.to_string(),
        name: name.to_string(),
    }));
    // SAFETY: ctx stays valid until on_cancel frees it.
    unsafe { shared.source.set_context(ctx.cast()) };
    shared.source.set_event_handler_f(on_flip);
    shared.source.set_cancel_handler_f(on_cancel);
    // Activated now so every failure path below can simply cancel it.
    shared.source.resume();
    shared
}

struct Running {
    shared: Arc<Shared>,
    proc_id: AudioDeviceIOProcID,
}

pub struct ZeroSignalDetector {
    device_id: u32,
    uid: String,
    name: String,
    sample_rate: f64,
    zero_seconds: f64,
    tx: Sender<Event>,
    running: Option<Running>,
}

impl ZeroSignalDetector {
    pub fn new(device: &AudioDevice, sample_rate: f64, zero_seconds: f64, tx: Sender<Event>) -> Self {
        Self {
            device_id: device.id,
            uid: device.uid.clone(),
            name: device.name.clone(),
            sample_rate,
            zero_seconds,
            tx,
            running: None,
        }
    }
}

impl LivenessDetecting for ZeroSignalDetector {
    fn device_id(&self) -> u32 {
        self.device_id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn zero_seconds(&self) -> f64 {
        self.zero_seconds
    }

    /// False when the device cannot be opened (no permission, or gone since the snapshot).
    fn start(&mut self) -> bool {
        if self.running.is_some() {
            return true;
        }
        let bytes_per_sample = prepare_liveness_input(self.device_id);
        let threshold = (self.sample_rate * self.zero_seconds) as usize;
        let shared = flip_channel(threshold, bytes_per_sample, &self.uid, &self.name, self.tx.clone());

        let mut proc_id: AudioDeviceIOProcID = None;
        // SAFETY: `shared` outlives the proc: it is only released after Stop and Destroy.
        let status = unsafe {
            AudioDeviceCreateIOProcID(
                self.device_id,
                Some(io_proc),
                Arc::as_ptr(&shared).cast_mut().cast(),
                NonNull::from(&mut proc_id),
            )
        };
        if status != 0 || proc_id.is_none() {
            shared.cancel();
            return false;
        }
        // SAFETY: proc_id was just created on this device.
        if unsafe { AudioDeviceStart(self.device_id, proc_id) } != 0 {
            // SAFETY: as above.
            unsafe { AudioDeviceDestroyIOProcID(self.device_id, proc_id) };
            shared.cancel();
            return false;
        }
        self.running = Some(Running { shared, proc_id });
        true
    }

    fn stop(&mut self) {
        let Some(running) = self.running.take() else { return };
        // SAFETY: the proc was created and started by `start`. Destroy returns once the HAL no
        // longer calls it (the same assumption the Swift build makes).
        unsafe {
            AudioDeviceStop(self.device_id, running.proc_id);
            AudioDeviceDestroyIOProcID(self.device_id, running.proc_id);
        }
        running.shared.cancel();
    }
}

impl Drop for ZeroSignalDetector {
    fn drop(&mut self) {
        self.stop();
    }
}
