//! The only code that runs on the HAL's realtime IO thread. Everything between the REALTIME
//! markers allocates nothing, takes no lock and dispatches nothing; `tests/realtime_discipline.rs`
//! reads that span and `tests/realtime_ioproc.rs` counts allocations across 100,000 calls.
#![deny(
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::integer_division,
    clippy::arithmetic_side_effects
)]

use std::cell::UnsafeCell;
use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicI32, Ordering};

use dispatch2::{DispatchRetained, DispatchSource};
use objc2_core_audio_types::{AudioBuffer, AudioBufferList, AudioTimeStamp};

use super::streak::{Flip, ZeroStreak};

/// What the IOProc reads and writes. Built before `AudioDeviceStart` and kept alive by the
/// detector and by the flip source's context until the source's cancel handler has run.
pub struct Shared {
    /// Touched by the IO thread only, which is serial with itself.
    pub(crate) streak: UnsafeCell<ZeroStreak>,
    /// Fixed before the IOProc starts.
    pub(crate) bytes_per_sample: usize,
    /// The verdict word: 1 silent, 0 live. Stored before `merge_data`, read by the source handler.
    pub(crate) silent: AtomicI32,
    /// A user-data-add source, Apple's channel out of a realtime thread.
    pub(crate) source: DispatchRetained<DispatchSource>,
}

// SAFETY: `streak` is only touched from the IO thread (one IOProc, serial with itself); every
// other field is immutable or atomic, and DispatchSource is thread-safe.
unsafe impl Sync for Shared {}
// SAFETY: as above.
unsafe impl Send for Shared {}

impl Shared {
    /// Stops the flip source; the cancel handler then releases its hold on this.
    pub fn cancel(&self) {
        self.source.cancel();
    }
}

// REALTIME BEGIN
/// The IOProc. `client` is a `*const Shared`.
///
/// # Safety
/// `input` must point at a valid buffer list and `client` at a live `Shared`.
pub unsafe extern "C-unwind" fn io_proc(
    _device: u32,
    _now: NonNull<AudioTimeStamp>,
    input: NonNull<AudioBufferList>,
    _input_time: NonNull<AudioTimeStamp>,
    _output: NonNull<AudioBufferList>,
    _output_time: NonNull<AudioTimeStamp>,
    client: *mut c_void,
) -> i32 {
    // SAFETY: the detector passes a pointer to the Shared it keeps alive while the proc exists.
    if let Some(shared) = unsafe { client.cast::<Shared>().as_ref() } {
        // SAFETY: the HAL hands a valid list for the duration of the call.
        unsafe { consume(shared, input.as_ptr()) };
    }
    0
}

/// One buffer list: scan for a non-zero byte, feed the streak, signal only on a change of verdict.
///
/// # Safety
/// `list` must point at a valid buffer list whose `mNumberBuffers` entries are readable.
pub unsafe fn consume(shared: &Shared, list: *const AudioBufferList) {
    // SAFETY: caller guarantees the list; buffers are read in place, never copied.
    let buffers = unsafe {
        let count = (*list).mNumberBuffers as usize;
        let first = std::ptr::addr_of!((*list).mBuffers).cast::<AudioBuffer>();
        std::slice::from_raw_parts(first, count)
    };
    let Some(first) = buffers.first() else { return };
    // Frames, not bytes: interleaved devices report one buffer of N*channels samples,
    // non-interleaved one buffer per channel.
    let Some(frame_bytes) = shared.bytes_per_sample.checked_mul(first.mNumberChannels as usize) else {
        return;
    };
    let Some(frames) = (first.mDataByteSize as usize).checked_div(frame_bytes) else { return };

    let mut all_zero = true;
    for buffer in buffers {
        if buffer.mData.is_null() || buffer.mDataByteSize == 0 {
            continue;
        }
        if contains_non_zero_byte(buffer.mData.cast::<u8>(), buffer.mDataByteSize as usize) {
            all_zero = false;
            break;
        }
    }

    // SAFETY: only this thread touches the streak.
    let streak = unsafe { &mut *shared.streak.get() };
    let Some(flip) = streak.feed(frames, all_zero) else { return };
    shared.silent.store(i32::from(flip == Flip::BecameSilent), Ordering::Release);
    shared.source.merge_data(1);
}

/// Any non-zero byte is signal: independent of the sample format, and a float -0.0 errs towards
/// "live", which is how Cleat behaved before liveness existed.
fn contains_non_zero_byte(ptr: *const u8, len: usize) -> bool {
    let mut offset: usize = 0;
    while let Some(end) = offset.checked_add(8) {
        if end > len {
            break;
        }
        // SAFETY: offset..end lies within the buffer the HAL handed us.
        if unsafe { ptr.add(offset).cast::<u64>().read_unaligned() } != 0 {
            return true;
        }
        offset = end;
    }
    while offset < len {
        // SAFETY: offset < len.
        if unsafe { ptr.add(offset).read() } != 0 {
            return true;
        }
        match offset.checked_add(1) {
            Some(next) => offset = next,
            None => break,
        }
    }
    false
}
// REALTIME END
