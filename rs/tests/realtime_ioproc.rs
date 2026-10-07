//! Proof one of three (plan §4.2-1): the IOProc allocates nothing. A counting global allocator
//! counts allocations made on this thread while `io_proc` runs 100,000 times over a real flip
//! source, through silent, live, silent. Flips arrive on the channel and are read afterwards.
//!
//! Counted per thread on purpose: the flip source's handler runs on a GCD thread and allocates
//! (it sends an Event); that is the non-realtime side. Named limit: `dispatch_source_merge_data`
//! may allocate inside libdispatch with its own malloc, which this allocator does not see.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::mpsc::channel;
use std::time::Duration;

use cleat_rs::engine::Event;
use cleat_rs::liveness::flip_channel;
use cleat_rs::liveness::ioproc::io_proc;
use objc2_core_audio_types::{AudioBuffer, AudioBufferList, AudioTimeStamp};

struct Counting;

thread_local! {
    static COUNTING: Cell<bool> = const { Cell::new(false) };
    /// Per thread, so tests running in parallel do not add to each other's count.
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}

fn note_allocation() {
    if COUNTING.try_with(|c| c.get()).unwrap_or(false) {
        let _ = ALLOCATIONS.try_with(|n| n.set(n.get() + 1));
    }
}

fn allocations() -> usize {
    ALLOCATIONS.with(|n| n.get())
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        note_allocation();
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        note_allocation();
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        note_allocation();
        unsafe { System.realloc(ptr, layout, new_size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

const FRAMES: usize = 4096;

/// Two buffers' worth of room; `mNumberBuffers` says how many are real.
#[repr(C)]
struct TwoBufferList {
    count: u32,
    buffers: [AudioBuffer; 2],
}

fn list(data: &mut [Vec<f32>], interleaved: bool) -> TwoBufferList {
    let channels = if interleaved { 2 } else { 1 };
    let mut buffers = [AudioBuffer { mNumberChannels: 0, mDataByteSize: 0, mData: std::ptr::null_mut() }; 2];
    for (b, d) in buffers.iter_mut().zip(data.iter_mut()) {
        *b = AudioBuffer {
            mNumberChannels: channels,
            mDataByteSize: (d.len() * 4) as u32,
            mData: d.as_mut_ptr().cast::<c_void>(),
        };
    }
    TwoBufferList { count: data.len() as u32, buffers }
}

/// Runs `calls` IOProc calls per phase (silent, live, silent) and returns (allocations, flips).
fn run(interleaved: bool, calls_per_phase: [usize; 3]) -> (usize, Vec<bool>) {
    let (tx, rx) = channel();
    // Threshold: ten buffers of zeros.
    let shared = flip_channel(FRAMES * 10, 4, "uid", "Wireless microphone", tx);
    let samples = if interleaved { FRAMES * 2 } else { FRAMES };
    let buffers = if interleaved { 1 } else { 2 };
    let mut zeros: Vec<Vec<f32>> = (0..buffers).map(|_| vec![0.0f32; samples]).collect();
    let mut noise: Vec<Vec<f32>> = (0..buffers).map(|_| vec![0.0f32; samples]).collect();
    // One non-zero sample at the very end, past the word-sized scan, in the last buffer.
    *noise.last_mut().unwrap().last_mut().unwrap() = 1e-6;
    let mut zero_list = list(&mut zeros, interleaved);
    let mut noise_list = list(&mut noise, interleaved);
    let zero_ptr = NonNull::from(&mut zero_list).cast::<AudioBufferList>();
    let noise_ptr = NonNull::from(&mut noise_list).cast::<AudioBufferList>();
    let mut ts: AudioTimeStamp = unsafe { std::mem::zeroed() };
    let ts = NonNull::from(&mut ts);
    let client = std::sync::Arc::as_ptr(&shared).cast_mut().cast::<c_void>();

    let before = allocations();
    COUNTING.with(|c| c.set(true));
    for (phase, calls) in calls_per_phase.iter().enumerate() {
        let input = if phase == 1 { noise_ptr } else { zero_ptr };
        for _ in 0..*calls {
            unsafe { io_proc(1, ts, input, ts, input, ts, client) };
        }
        // Lets the source's handler run between flips, so DATA_ADD does not coalesce them.
        std::thread::sleep(Duration::from_millis(50));
    }
    COUNTING.with(|c| c.set(false));
    let allocations = allocations() - before;

    std::thread::sleep(Duration::from_millis(100));
    let flips = rx
        .try_iter()
        .map(|e| match e {
            Event::LivenessFlip { live, .. } => live,
            _ => panic!("unexpected event"),
        })
        .collect();
    shared.cancel();
    (allocations, flips)
}

#[test]
fn io_proc_allocates_nothing_over_100k_interleaved_calls() {
    let (allocations, flips) = run(true, [30_000, 30_000, 40_000]);
    assert_eq!(allocations, 0, "the IOProc allocated {allocations} times");
    assert_eq!(flips, vec![false, true, false], "flips (live?) in order");
}

#[test]
fn io_proc_allocates_nothing_on_non_interleaved_buffers() {
    let (allocations, flips) = run(false, [2_000, 2_000, 2_000]);
    assert_eq!(allocations, 0, "the IOProc allocated {allocations} times");
    assert_eq!(flips, vec![false, true, false]);
}

/// Control: the counter does see an allocation on this thread, so zero above means something.
#[test]
fn the_counter_sees_an_allocation() {
    let before = allocations();
    COUNTING.with(|c| c.set(true));
    let v = std::hint::black_box(vec![1u8; 16]);
    COUNTING.with(|c| c.set(false));
    drop(v);
    assert!(allocations() > before);
}
