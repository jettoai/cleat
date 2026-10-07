//! Thin, non-throwing wrappers over the CoreAudio property calls. Reads return `Option`, writes
//! the raw `OSStatus`. `set` is the only call site of `AudioObjectSetPropertyData` in the crate.

use std::ffi::c_void;
use std::mem::size_of;
use std::ptr::{null, NonNull};

use objc2_core_audio::{
    kAudioHardwarePropertyDevices, kAudioObjectPropertyElementMain, kAudioObjectPropertyScopeGlobal,
    kAudioObjectSystemObject, AudioObjectGetPropertyData, AudioObjectGetPropertyDataSize, AudioObjectID,
    AudioObjectIsPropertySettable, AudioObjectPropertyAddress, AudioObjectPropertyElement,
    AudioObjectPropertyScope, AudioObjectPropertySelector, AudioObjectSetPropertyData,
    kAudioDevicePropertyStreamConfiguration,
};
use objc2_core_audio_types::{AudioBuffer, AudioBufferList};
use objc2_core_foundation::{CFRetained, CFString};

pub const SYSTEM_OBJECT: AudioObjectID = kAudioObjectSystemObject as AudioObjectID;

pub fn address(
    selector: AudioObjectPropertySelector,
    scope: AudioObjectPropertyScope,
    element: AudioObjectPropertyElement,
) -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress { mSelector: selector, mScope: scope, mElement: element }
}

pub fn global(selector: AudioObjectPropertySelector) -> AudioObjectPropertyAddress {
    address(selector, kAudioObjectPropertyScopeGlobal, kAudioObjectPropertyElementMain)
}

/// Read a fixed-size POD property.
pub fn get<T: Copy + Default>(object: AudioObjectID, addr: AudioObjectPropertyAddress) -> Option<T> {
    let mut addr = addr;
    let mut value = T::default();
    let mut size = size_of::<T>() as u32;
    // SAFETY: every pointer refers to a live local of the stated size.
    let status = unsafe {
        AudioObjectGetPropertyData(
            object,
            NonNull::from(&mut addr),
            0,
            null(),
            NonNull::from(&mut size),
            NonNull::from(&mut value).cast(),
        )
    };
    (status == 0).then_some(value)
}

/// Name and UID properties come back as a retained CFString.
pub fn get_string(object: AudioObjectID, addr: AudioObjectPropertyAddress) -> Option<String> {
    let mut addr = addr;
    let mut raw: *const CFString = null();
    let mut size = size_of::<*const CFString>() as u32;
    // SAFETY: `raw` is a pointer-sized out-buffer.
    let status = unsafe {
        AudioObjectGetPropertyData(
            object,
            NonNull::from(&mut addr),
            0,
            null(),
            NonNull::from(&mut size),
            NonNull::from(&mut raw).cast(),
        )
    };
    let ptr = NonNull::new(raw as *mut CFString).filter(|_| status == 0)?;
    // SAFETY: the HAL hands back a +1 reference, which CFRetained takes over.
    let s = unsafe { CFRetained::from_raw(ptr) };
    Some(s.to_string())
}

fn data_size(object: AudioObjectID, addr: AudioObjectPropertyAddress) -> Option<u32> {
    let mut addr = addr;
    let mut size = 0u32;
    // SAFETY: valid locals.
    let status = unsafe {
        AudioObjectGetPropertyDataSize(object, NonNull::from(&mut addr), 0, null(), NonNull::from(&mut size))
    };
    (status == 0 && size > 0).then_some(size)
}

/// Every device id the HAL knows about.
pub fn device_ids() -> Vec<u32> {
    let mut addr = global(kAudioHardwarePropertyDevices);
    let Some(mut size) = data_size(SYSTEM_OBJECT, addr) else { return vec![] };
    let mut ids = vec![0u32; size as usize / size_of::<u32>()];
    // SAFETY: `ids` holds `size` bytes.
    let status = unsafe {
        AudioObjectGetPropertyData(
            SYSTEM_OBJECT,
            NonNull::from(&mut addr),
            0,
            null(),
            NonNull::from(&mut size),
            NonNull::new(ids.as_mut_ptr().cast::<c_void>()).expect("vec pointer"),
        )
    };
    if status != 0 {
        return vec![];
    }
    ids.truncate(size as usize / size_of::<u32>());
    ids
}

/// Channels on one side of a device: what makes a device an input or an output.
pub fn channel_count(device: AudioObjectID, scope: AudioObjectPropertyScope) -> u32 {
    let mut addr = address(kAudioDevicePropertyStreamConfiguration, scope, kAudioObjectPropertyElementMain);
    let Some(mut size) = data_size(device, addr) else { return 0 };
    // u64 storage keeps the buffer aligned for AudioBufferList.
    let mut buf = vec![0u64; (size as usize).div_ceil(8)];
    // SAFETY: `buf` holds at least `size` bytes.
    let status = unsafe {
        AudioObjectGetPropertyData(
            device,
            NonNull::from(&mut addr),
            0,
            null(),
            NonNull::from(&mut size),
            NonNull::new(buf.as_mut_ptr().cast::<c_void>()).expect("vec pointer"),
        )
    };
    if status != 0 || (size as usize) < size_of::<u32>() {
        return 0;
    }
    let list = buf.as_ptr().cast::<AudioBufferList>();
    // SAFETY: the HAL filled an AudioBufferList of `size` bytes; buffers are read within it.
    unsafe {
        let n = (*list).mNumberBuffers as usize;
        let first = std::ptr::addr_of!((*list).mBuffers).cast::<AudioBuffer>();
        let fits = (size as usize).saturating_sub(std::mem::offset_of!(AudioBufferList, mBuffers)) / size_of::<AudioBuffer>();
        (0..n.min(fits)).map(|i| (*first.add(i)).mNumberChannels).sum()
    }
}

pub fn is_settable(object: AudioObjectID, addr: AudioObjectPropertyAddress) -> bool {
    let mut addr = addr;
    let mut settable: u8 = 0;
    // SAFETY: valid locals.
    let status =
        unsafe { AudioObjectIsPropertySettable(object, NonNull::from(&mut addr), NonNull::from(&mut settable)) };
    status == 0 && settable != 0
}

/// The one CoreAudio write in the crate. Only `core_audio.rs` calls it.
pub(super) fn set<T: Copy>(object: AudioObjectID, addr: AudioObjectPropertyAddress, value: T) -> i32 {
    let mut addr = addr;
    let mut value = value;
    // SAFETY: `value` is a live POD local of the stated size.
    unsafe {
        AudioObjectSetPropertyData(
            object,
            NonNull::from(&mut addr),
            0,
            null(),
            size_of::<T>() as u32,
            NonNull::from(&mut value).cast(),
        )
    }
}
