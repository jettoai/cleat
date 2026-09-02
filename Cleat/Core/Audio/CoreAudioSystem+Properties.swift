import AudioToolbox
import CoreAudio
import Foundation

/// Thin, non-throwing wrappers over the four CoreAudio property calls. Every read returns an
/// optional and every write returns the raw `OSStatus`: a device that vanished mid-pass is an
/// ordinary outcome here, not an error condition, and the engine decides what to do about it.
enum AudioProperty {

    static func address(
        _ selector: AudioObjectPropertySelector,
        scope: AudioObjectPropertyScope = kAudioObjectPropertyScopeGlobal,
        element: AudioObjectPropertyElement = kAudioObjectPropertyElementMain
    ) -> AudioObjectPropertyAddress {
        AudioObjectPropertyAddress(mSelector: selector, mScope: scope, mElement: element)
    }

    /// Read a fixed-size POD property (`UInt32`, `Float32`, `Float64`, `AudioDeviceID`).
    static func value<T>(_ object: AudioObjectID, _ address: AudioObjectPropertyAddress, as type: T.Type) -> T? {
        var address = address
        var size = UInt32(MemoryLayout<T>.size)
        let buffer = UnsafeMutablePointer<T>.allocate(capacity: 1)
        defer { buffer.deallocate() }
        guard AudioObjectGetPropertyData(object, &address, 0, nil, &size, buffer) == noErr else { return nil }
        return buffer.pointee
    }

    /// The value is handed over as raw bytes: every property written here is a POD scalar, and
    /// taking a typed pointer to a generic would have the compiler assume it might hold a
    /// reference.
    static func setValue<T>(_ object: AudioObjectID, _ address: AudioObjectPropertyAddress, _ value: T) -> OSStatus {
        var address = address
        return withUnsafeBytes(of: value) { bytes in
            guard let base = bytes.baseAddress else { return kAudio_ParamError }
            return AudioObjectSetPropertyData(object, &address, 0, nil, UInt32(bytes.count), base)
        }
    }

    /// CoreAudio hands back a retained CFString for name and UID properties.
    static func string(_ object: AudioObjectID, _ address: AudioObjectPropertyAddress) -> String? {
        var address = address
        var size = UInt32(MemoryLayout<CFString?>.size)
        var result: Unmanaged<CFString>?
        let status = withUnsafeMutablePointer(to: &result) {
            AudioObjectGetPropertyData(object, &address, 0, nil, &size, $0)
        }
        guard status == noErr, let result else { return nil }
        return result.takeRetainedValue() as String
    }

    /// Every device id the HAL knows about.
    static func deviceIDs() -> [AudioDeviceID] {
        var address = address(kAudioHardwarePropertyDevices)
        var size: UInt32 = 0
        let system = AudioObjectID(kAudioObjectSystemObject)
        guard AudioObjectGetPropertyDataSize(system, &address, 0, nil, &size) == noErr, size > 0 else {
            return []
        }
        var ids = [AudioDeviceID](repeating: 0, count: Int(size) / MemoryLayout<AudioDeviceID>.size)
        guard AudioObjectGetPropertyData(system, &address, 0, nil, &size, &ids) == noErr else { return [] }
        return ids
    }

    /// Channels on one side of a device. This, not the device's name or transport type, is what
    /// makes a device an input or an output: a headset reports both, a webcam only input.
    static func channelCount(_ device: AudioDeviceID, scope: AudioObjectPropertyScope) -> Int {
        var address = address(kAudioDevicePropertyStreamConfiguration, scope: scope)
        var size: UInt32 = 0
        guard AudioObjectGetPropertyDataSize(device, &address, 0, nil, &size) == noErr, size > 0 else {
            return 0
        }
        let raw = UnsafeMutableRawPointer.allocate(
            byteCount: Int(size), alignment: MemoryLayout<AudioBufferList>.alignment
        )
        defer { raw.deallocate() }
        guard AudioObjectGetPropertyData(device, &address, 0, nil, &size, raw) == noErr else { return 0 }
        let list = UnsafeMutableAudioBufferListPointer(raw.assumingMemoryBound(to: AudioBufferList.self))
        return list.reduce(0) { $0 + Int($1.mNumberChannels) }
    }

    static func isSettable(_ object: AudioObjectID, _ address: AudioObjectPropertyAddress) -> Bool {
        var address = address
        var settable: DarwinBoolean = false
        guard AudioObjectIsPropertySettable(object, &address, &settable) == noErr else { return false }
        return settable.boolValue
    }
}

/// A registered property listener, kept so it can be removed again - `AudioObjectRemoveProperty
/// ListenerBlock` needs the very same block, address and queue that registered it.
final class ListenerToken: @unchecked Sendable {
    let object: AudioObjectID
    var address: AudioObjectPropertyAddress
    let queue: DispatchQueue
    let block: AudioObjectPropertyListenerBlock

    init(
        object: AudioObjectID,
        address: AudioObjectPropertyAddress,
        queue: DispatchQueue,
        block: @escaping AudioObjectPropertyListenerBlock
    ) {
        self.object = object
        self.address = address
        self.queue = queue
        self.block = block
    }
}
