import AudioToolbox
import CoreAudio
import Foundation

/// Everything the engine is allowed to do to the audio system. A protocol so the engine can be
/// reasoned about (and, in future, tested) without a sound card; `CoreAudioSystem` is the only
/// implementation that ships.
protocol AudioSystem: AnyObject {
    /// One consistent reading. `config` is passed in because the volumes worth reading are exactly
    /// the ones the config asks about - the named devices, or every input device when the config
    /// carries the `"*"` wildcard. Reading more than that is IPC we would throw away.
    func snapshot(config: Config) -> DeviceSnapshot

    func setDefaultInput(_ device: AudioDeviceID) -> OSStatus
    func setDefaultOutput(_ device: AudioDeviceID) -> OSStatus
    func setBalance(_ device: AudioDeviceID, _ value: Float) -> OSStatus
    func setInputVolume(_ device: AudioDeviceID, _ value: Float) -> OSStatus

    func nominalSampleRate(_ device: AudioDeviceID) -> Double?

    func addSystemListener(
        selector: AudioObjectPropertySelector,
        queue: DispatchQueue,
        block: @escaping () -> Void
    ) -> ListenerToken

    func addDeviceListener(
        device: AudioDeviceID,
        selector: AudioObjectPropertySelector,
        scope: AudioObjectPropertyScope,
        element: AudioObjectPropertyElement,
        queue: DispatchQueue,
        block: @escaping () -> Void
    ) -> ListenerToken

    func removeListener(_ token: ListenerToken)
}

/// The real HAL.
///
/// Balance is read and written through `kAudioHardwareServiceDeviceProperty_VirtualMainBalance`
/// (AudioToolbox/AudioHardwareService.h). It is the property System Settings' balance slider
/// moves, and - unlike stereo pan - it posts a change notification, which is the whole reason
/// Cleat can hold the balance without polling the way the Hammerspoon version had to.
final class CoreAudioSystem: AudioSystem, @unchecked Sendable {

    private static let systemObject = AudioObjectID(kAudioObjectSystemObject)

    // MARK: - Reading

    func snapshot(config: Config) -> DeviceSnapshot {
        let devices = AudioProperty.deviceIDs().compactMap(describe)
        let defaultInput = defaultDevice(kAudioHardwarePropertyDefaultInputDevice)
        let defaultOutput = defaultDevice(kAudioHardwarePropertyDefaultOutputDevice)

        // Exactly the devices the input volume rule has a target for: the read set and the
        // enforced set are the same question, asked in one place.
        let worthReading = config.inputVolumeDevices(among: devices)

        var inputVolumes: [AudioDeviceID: Float] = [:]
        for device in worthReading {
            if let volume = inputVolume(device.id) {
                inputVolumes[device.id] = volume
            }
        }

        // Only the reclaim rule asks whether the Mac is playing, so only a config that turns it on
        // pays for the read.
        let outputRunning = config.reclaim.isEmpty
            ? false
            : defaultOutput.map(isRunningSomewhere) ?? false

        return DeviceSnapshot(
            devices: devices,
            defaultInput: defaultInput,
            defaultOutput: defaultOutput,
            outputBalance: defaultOutput.flatMap(balance),
            outputRunning: outputRunning,
            inputVolumes: inputVolumes,
            liveness: [:],  // filled in by the engine, which owns the detectors
            arrived: []     // and so is this: only the engine remembers the previous pass
        )
    }

    private func describe(_ id: AudioDeviceID) -> AudioDevice? {
        guard let name = AudioProperty.string(id, AudioProperty.address(kAudioObjectPropertyName)),
              let uid = AudioProperty.string(id, AudioProperty.address(kAudioDevicePropertyDeviceUID))
        else { return nil }

        return AudioDevice(
            id: id,
            name: name,
            uid: uid,
            hasInput: AudioProperty.channelCount(id, scope: kAudioObjectPropertyScopeInput) > 0,
            hasOutput: AudioProperty.channelCount(id, scope: kAudioObjectPropertyScopeOutput) > 0,
            transport: AudioProperty.value(
                id, AudioProperty.address(kAudioDevicePropertyTransportType), as: UInt32.self
            ) ?? kAudioDeviceTransportTypeUnknown
        )
    }

    private func defaultDevice(_ selector: AudioObjectPropertySelector) -> AudioDeviceID? {
        guard let id = AudioProperty.value(
            Self.systemObject, AudioProperty.address(selector), as: AudioDeviceID.self
        ), id != kAudioObjectUnknown else { return nil }
        return id
    }

    func nominalSampleRate(_ device: AudioDeviceID) -> Double? {
        AudioProperty.value(
            device,
            AudioProperty.address(kAudioDevicePropertyNominalSampleRate, scope: kAudioObjectPropertyScopeInput),
            as: Float64.self
        )
    }

    private func balance(_ device: AudioDeviceID) -> Float? {
        AudioProperty.value(device, Self.balanceAddress, as: Float32.self)
    }

    /// Whether any process is running IO on this device. "Somewhere" is the point: it counts other
    /// applications, which is what makes it the answer to "is the Mac playing", rather than
    /// `kAudioDevicePropertyDeviceIsRunning`, which only sees this process. A device that will not
    /// answer reads as not playing, so the reclaim rule stays quiet rather than guessing.
    private func isRunningSomewhere(_ device: AudioDeviceID) -> Bool {
        let value = AudioProperty.value(
            device, AudioProperty.address(kAudioDevicePropertyDeviceIsRunningSomewhere), as: UInt32.self
        )
        return (value ?? 0) != 0
    }

    /// Main element first; devices that expose no main volume are read as the mean of the two
    /// channels, which is what System Settings shows for them.
    private func inputVolume(_ device: AudioDeviceID) -> Float? {
        if let value = AudioProperty.value(device, Self.volumeAddress(element: kAudioObjectPropertyElementMain), as: Float32.self) {
            return value
        }
        let channels = [AudioObjectPropertyElement(1), AudioObjectPropertyElement(2)].compactMap {
            AudioProperty.value(device, Self.volumeAddress(element: $0), as: Float32.self)
        }
        guard !channels.isEmpty else { return nil }
        return channels.reduce(0, +) / Float(channels.count)
    }

    private static let balanceAddress = AudioProperty.address(
        kAudioHardwareServiceDeviceProperty_VirtualMainBalance,
        scope: kAudioObjectPropertyScopeOutput
    )

    private static func volumeAddress(element: AudioObjectPropertyElement) -> AudioObjectPropertyAddress {
        AudioProperty.address(
            kAudioDevicePropertyVolumeScalar, scope: kAudioObjectPropertyScopeInput, element: element
        )
    }

    // MARK: - Writing

    func setDefaultInput(_ device: AudioDeviceID) -> OSStatus {
        AudioProperty.setValue(
            Self.systemObject, AudioProperty.address(kAudioHardwarePropertyDefaultInputDevice), device
        )
    }

    func setDefaultOutput(_ device: AudioDeviceID) -> OSStatus {
        AudioProperty.setValue(
            Self.systemObject, AudioProperty.address(kAudioHardwarePropertyDefaultOutputDevice), device
        )
    }

    func setBalance(_ device: AudioDeviceID, _ value: Float) -> OSStatus {
        AudioProperty.setValue(device, Self.balanceAddress, Float32(value))
    }

    /// Main element when the device has one, otherwise both channels. Writing channels separately
    /// is what keeps a two-channel USB mic from ending up with one side at the old gain.
    func setInputVolume(_ device: AudioDeviceID, _ value: Float) -> OSStatus {
        let main = Self.volumeAddress(element: kAudioObjectPropertyElementMain)
        if AudioProperty.isSettable(device, main) {
            return AudioProperty.setValue(device, main, Float32(value))
        }

        var lastError: OSStatus = kAudioHardwareUnknownPropertyError
        var wroteOne = false
        for channel in [AudioObjectPropertyElement(1), AudioObjectPropertyElement(2)] {
            let address = Self.volumeAddress(element: channel)
            guard AudioProperty.isSettable(device, address) else { continue }
            let status = AudioProperty.setValue(device, address, Float32(value))
            if status == noErr { wroteOne = true } else { lastError = status }
        }
        return wroteOne ? noErr : lastError
    }

    // MARK: - Listeners

    func addSystemListener(
        selector: AudioObjectPropertySelector,
        queue: DispatchQueue,
        block: @escaping () -> Void
    ) -> ListenerToken {
        register(
            object: Self.systemObject,
            address: AudioProperty.address(selector),
            queue: queue,
            block: block
        )
    }

    func addDeviceListener(
        device: AudioDeviceID,
        selector: AudioObjectPropertySelector,
        scope: AudioObjectPropertyScope,
        element: AudioObjectPropertyElement,
        queue: DispatchQueue,
        block: @escaping () -> Void
    ) -> ListenerToken {
        register(
            object: device,
            address: AudioProperty.address(selector, scope: scope, element: element),
            queue: queue,
            block: block
        )
    }

    private func register(
        object: AudioObjectID,
        address: AudioObjectPropertyAddress,
        queue: DispatchQueue,
        block: @escaping () -> Void
    ) -> ListenerToken {
        // The address CoreAudio reports back is ignored: every listener here is registered for one
        // selector, so which one fired is already known from the token it was created with.
        let listener: AudioObjectPropertyListenerBlock = { _, _ in block() }
        var address = address
        AudioObjectAddPropertyListenerBlock(object, &address, queue, listener)
        return ListenerToken(object: object, address: address, queue: queue, block: listener)
    }

    func removeListener(_ token: ListenerToken) {
        var address = token.address
        AudioObjectRemovePropertyListenerBlock(token.object, &address, token.queue, token.block)
    }
}
