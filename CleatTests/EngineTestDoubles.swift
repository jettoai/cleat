import CoreAudio
import Foundation
@testable import Cleat

// MARK: - Doubles

/// Every value the engine passed to its error-reporting callback, in order. Written on the engine
/// queue and read after `drain`, like `DetectorLog`, so it needs no lock.
final class ReportLog: @unchecked Sendable {
    private(set) var values: [Bool] = []
    func append(_ value: Bool) { values.append(value) }
}

/// The audio system as a value. Writes land back in the snapshot, so a second reconcile finds the
/// state already held - the same reason the real engine converges instead of writing every beat.
final class FakeAudioSystem: AudioSystem, @unchecked Sendable {

    var snapshotValue: DeviceSnapshot
    var sampleRate: Double? = 48_000
    /// CoreAudio answers `noErr` for a default-output write against a device that has just
    /// appeared and then quietly does not move the output. Setting this to false is that device.
    var outputWritesStick = true
    private(set) var writes: [String] = []
    /// Device listener blocks by selector, so a test can play the property change CoreAudio
    /// would deliver. Call `fire` on the engine queue, where CoreAudio delivers them.
    private var deviceListeners: [(AudioObjectPropertySelector, () -> Void)] = []

    init(snapshot: DeviceSnapshot) {
        self.snapshotValue = snapshot
    }

    func snapshot(config: Config) -> DeviceSnapshot { snapshotValue }

    func setDefaultInput(_ device: AudioDeviceID) -> OSStatus {
        writes.append("input:\(device)")
        snapshotValue.defaultInput = device
        return noErr
    }

    func setDefaultOutput(_ device: AudioDeviceID) -> OSStatus {
        writes.append("output:\(device)")
        if outputWritesStick { snapshotValue.defaultOutput = device }
        return noErr
    }

    func setBalance(_ device: AudioDeviceID, _ value: Float) -> OSStatus {
        writes.append("balance:\(device):\(String(format: "%.2f", value))")
        snapshotValue.outputBalance = value
        return noErr
    }

    func setInputVolume(_ device: AudioDeviceID, _ value: Float) -> OSStatus {
        writes.append("volume:\(device):\(String(format: "%.2f", value))")
        snapshotValue.inputVolumes[device] = value
        return noErr
    }

    func setOutputVolume(_ device: AudioDeviceID, _ value: Float) -> OSStatus {
        writes.append("outvol:\(device):\(String(format: "%.4f", value))")
        let channels = max(snapshotValue.outputVolumes.count, 2)
        snapshotValue.outputVolumes = Array(repeating: value, count: channels)
        return noErr
    }

    func nominalSampleRate(_ device: AudioDeviceID) -> Double? { sampleRate }

    func addSystemListener(
        selector: AudioObjectPropertySelector,
        queue: DispatchQueue,
        block: @escaping () -> Void
    ) -> ListenerToken {
        ListenerToken(
            object: 0, address: AudioProperty.address(selector), queue: queue, block: { _, _ in }
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
        deviceListeners.append((selector, block))
        return ListenerToken(
            object: device,
            address: AudioProperty.address(selector, scope: scope, element: element),
            queue: queue,
            block: { _, _ in }
        )
    }

    func removeListener(_ token: ListenerToken) {}

    func fire(_ selector: AudioObjectPropertySelector) {
        for (registered, block) in deviceListeners where registered == selector { block() }
    }
}

/// The `log stream` reader as a script: a test hands it events and exits on the engine queue.
final class FakeVolumeWriterSource: VolumeWriterSource, @unchecked Sendable {
    var startResult = true
    private(set) var startCount = 0
    private(set) var stopCount = 0
    private var onEvent: (@Sendable (VolumeWriterEvent) -> Void)?
    private var onExit: (@Sendable (Int32) -> Void)?

    func start(queue: DispatchQueue,
               onEvent: @escaping @Sendable (VolumeWriterEvent) -> Void,
               onExit: @escaping @Sendable (Int32) -> Void) -> Bool {
        startCount += 1
        self.onEvent = onEvent
        self.onExit = onExit
        return startResult
    }

    func stop() { stopCount += 1 }

    /// Call on the engine queue, where the real reader delivers.
    func emit(_ event: VolumeWriterEvent) { onEvent?(event) }
    func exit(_ status: Int32) { onExit?(status) }
}

/// The HAL side of silence detection, as a script. "The device cannot be opened yet" is a value
/// here rather than a state of the machine, which is the whole point of injecting the factory.
final class DetectorLog: @unchecked Sendable {

    /// Answers for successive `start()` calls. The last one repeats once the script runs out.
    private let startResults: [Bool]
    private(set) var startCount = 0
    private(set) var stopCount = 0

    init(startResults: [Bool]) {
        self.startResults = startResults
    }

    func make(device: AudioDevice, zeroSeconds: Double) -> LivenessDetecting {
        FakeDetector(device: device, zeroSeconds: zeroSeconds, log: self)
    }

    fileprivate func nextStartResult() -> Bool {
        defer { startCount += 1 }
        guard startCount < startResults.count else { return startResults.last ?? true }
        return startResults[startCount]
    }

    fileprivate func recordStop() {
        stopCount += 1
    }
}

final class FakeDetector: LivenessDetecting, @unchecked Sendable {

    let deviceID: AudioDeviceID
    let name: String
    let zeroSeconds: Double
    private let log: DetectorLog

    init(device: AudioDevice, zeroSeconds: Double, log: DetectorLog) {
        self.deviceID = device.id
        self.name = device.name
        self.zeroSeconds = zeroSeconds
        self.log = log
    }

    func start() -> Bool { log.nextStartResult() }
    func stop() { log.recordStop() }
}
