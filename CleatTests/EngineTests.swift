import CoreAudio
import XCTest
@testable import Cleat

/// Engine-level behaviour: the bookkeeping that lives across events rather than inside one rule.
///
/// Each test drives a real `Engine` on its real queue with everything outside it replaced - the
/// audio system, the detectors, and all three files - so a decision is observed the same way the
/// daemon publishes it, through `status.json` and the log.
final class EngineTests: XCTestCase {

    private var directory: URL!
    private var configURL: URL!
    private var statusURL: URL!
    private var logURL: URL!
    private var engine: Engine!

    override func setUpWithError() throws {
        directory = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("cleat-engine-tests-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        configURL = directory.appendingPathComponent("config.json")
        statusURL = directory.appendingPathComponent("status.json")
        logURL = directory.appendingPathComponent("cleat.log")
    }

    override func tearDownWithError() throws {
        engine = nil
        try? FileManager.default.removeItem(at: directory)
    }

    // MARK: - Liveness retry

    /// A detector whose `start()` fails is tried again on the reconcile beats that already follow
    /// every device event, and says so in the log exactly once however many beats that takes.
    func testFailedDetectorStartIsRetriedOnLaterBeatsAndLoggedOnce() throws {
        let system = FakeAudioSystem(snapshot: DeviceSnapshot(
            devices: [Fixture.wireless, Fixture.brio],
            defaultInput: Fixture.wireless.id
        ))
        let detectors = DetectorLog(startResults: [false, false, false, true])
        let engine = try makeEngine(config: livenessOnly, system: system, detectors: detectors)

        engine.start(microphoneGranted: true, microphoneState: "authorized")
        drain(engine)

        // Two attempts already: the device rebind, then the reconcile that closes `start`.
        XCTAssertEqual(detectors.startCount, 2)
        XCTAssertEqual(runningDetectorUIDs(engine), [])

        engine.queue.sync { engine.reconcile() }
        XCTAssertEqual(detectors.startCount, 3)
        XCTAssertEqual(runningDetectorUIDs(engine), [])

        engine.queue.sync { engine.reconcile() }
        XCTAssertEqual(detectors.startCount, 4)
        XCTAssertEqual(runningDetectorUIDs(engine), [Fixture.wireless.uid])

        // A device that is measuring is left alone: no fifth attempt, no second line.
        engine.queue.sync { engine.reconcile() }
        XCTAssertEqual(detectors.startCount, 4)

        let log = logLines()
        XCTAssertEqual(log.filter { $0.contains("-> unavailable") }.count, 1)
        XCTAssertEqual(log.filter { $0.contains("-> measuring") }.count, 1)
        XCTAssertEqual(status()?.liveness["Wireless microphone"], "measuring")
    }

    /// The retry stops when the device does: a device that is no longer present is not attempted
    /// on every beat, and when it comes back its failure is worth logging again.
    func testAbsentDeviceIsNotRetriedAndReportsAgainOnReturn() throws {
        let system = FakeAudioSystem(snapshot: DeviceSnapshot(devices: [Fixture.brio]))
        let detectors = DetectorLog(startResults: [false])
        let engine = try makeEngine(config: livenessOnly, system: system, detectors: detectors)

        engine.start(microphoneGranted: true, microphoneState: "authorized")
        drain(engine)
        XCTAssertEqual(detectors.startCount, 0)
        XCTAssertEqual(status()?.liveness["Wireless microphone"], "absent")

        // Plugged in, and the HAL is not ready for it.
        engine.queue.sync {
            system.snapshotValue.devices = [Fixture.wireless, Fixture.brio]
            engine.reconcile()
        }
        XCTAssertEqual(detectors.startCount, 1)
        XCTAssertEqual(logLines().filter { $0.contains("-> unavailable") }.count, 1)

        // Unplugged again, then back: the second spell reports for itself.
        engine.queue.sync {
            system.snapshotValue.devices = [Fixture.brio]
            engine.reconcile()
            system.snapshotValue.devices = [Fixture.wireless, Fixture.brio]
            engine.reconcile()
        }
        XCTAssertEqual(detectors.startCount, 2)
        XCTAssertEqual(logLines().filter { $0.contains("-> unavailable") }.count, 2)
    }

    // MARK: - Microphone permission

    /// The four rules that need no microphone hold the desk while the TCC dialog is still on
    /// screen; the detectors attach when the answer arrives, and `cleat status` follows both.
    func testRulesRunBeforeThePermissionAnswerAndDetectorsFollowIt() throws {
        var config = livenessOnly
        config.balance = 0.5
        config.inputVolume = ["Wireless microphone": 88]

        let system = FakeAudioSystem(snapshot: DeviceSnapshot(
            devices: [Fixture.wireless, Fixture.displaySpeakers],
            defaultInput: Fixture.wireless.id,
            defaultOutput: Fixture.displaySpeakers.id,
            outputBalance: 0.2,
            inputVolumes: [Fixture.wireless.id: 0.6]
        ))
        let detectors = DetectorLog(startResults: [true])
        let engine = try makeEngine(config: config, system: system, detectors: detectors)

        engine.start(microphoneGranted: false, microphoneState: "not determined")
        drain(engine)

        XCTAssertEqual(system.writes, [
            "balance:\(Fixture.displaySpeakers.id):0.50",
            "volume:\(Fixture.wireless.id):0.88"
        ])
        XCTAssertEqual(detectors.startCount, 0)
        XCTAssertEqual(status()?.microphone, "not determined")
        XCTAssertEqual(
            status()?.liveness["Wireless microphone"], "disabled (no microphone permission)"
        )

        engine.updateMicrophone(granted: true, state: "authorized")
        drain(engine)

        XCTAssertEqual(detectors.startCount, 1)
        XCTAssertEqual(runningDetectorUIDs(engine), [Fixture.wireless.uid])
        XCTAssertEqual(status()?.microphone, "authorized")
        XCTAssertEqual(status()?.liveness["Wireless microphone"], "measuring")
        // Nothing was left to write the second time: the locks were already held.
        XCTAssertEqual(system.writes.count, 2)
        XCTAssertEqual(logLines().filter { $0.contains("microphone: authorized") }.count, 1)
    }

    /// A refusal tears nothing down, and is recorded where `cleat status` shows it.
    func testRefusedPermissionLeavesTheOtherRulesRunning() throws {
        let system = FakeAudioSystem(snapshot: DeviceSnapshot(
            devices: [Fixture.wireless, Fixture.airPods], defaultInput: Fixture.airPods.id
        ))
        let detectors = DetectorLog(startResults: [true])
        let engine = try makeEngine(config: livenessOnly, system: system, detectors: detectors)

        engine.start(microphoneGranted: false, microphoneState: "not determined")
        drain(engine)
        engine.updateMicrophone(granted: false, state: "denied")
        drain(engine)

        XCTAssertEqual(detectors.startCount, 0)
        XCTAssertEqual(status()?.microphone, "denied")
        XCTAssertEqual(
            status()?.liveness["Wireless microphone"], "disabled (no microphone permission)"
        )
        // The input pin rule still took the default input back off the blocked AirPods Max.
        XCTAssertEqual(system.writes, ["input:\(Fixture.wireless.id)"])
    }

    /// An answer that says what the engine already knew is not worth a line in the log.
    func testUnchangedPermissionIsANoOp() throws {
        let system = FakeAudioSystem(snapshot: DeviceSnapshot(
            devices: [Fixture.wireless], defaultInput: Fixture.wireless.id
        ))
        let detectors = DetectorLog(startResults: [true])
        let engine = try makeEngine(config: livenessOnly, system: system, detectors: detectors)

        engine.start(microphoneGranted: true, microphoneState: "authorized")
        drain(engine)
        let linesBefore = logLines().count

        engine.updateMicrophone(granted: true, state: "authorized")
        drain(engine)

        XCTAssertEqual(logLines().count, linesBefore)
        XCTAssertEqual(detectors.startCount, 1)
        XCTAssertEqual(detectors.stopCount, 0)
    }

    // MARK: - Helpers

    /// Albert's input pinning plus the one liveness entry, with the login item left out: the test
    /// host is a real app bundle, and `syncLaunchAtLogin` only stays out of SMAppService's way
    /// because the wanted state already matches.
    private var livenessOnly: Config {
        Config(
            input: Fixture.pinnedInput.input,
            blockedInput: Fixture.pinnedInput.blockedInput,
            liveness: Fixture.pinnedInput.liveness,
            launchAtLogin: false
        )
    }

    private func makeEngine(
        config: Config, system: FakeAudioSystem, detectors: DetectorLog
    ) throws -> Engine {
        try JSONEncoder().encode(config).write(to: configURL)
        let engine = Engine(
            system: system,
            log: EventLog(url: logURL, rotatedURL: directory.appendingPathComponent("cleat.log.1")),
            configURL: configURL,
            statusURL: statusURL,
            makeDetector: { device, _, zeroSeconds, _, _ in
                detectors.make(device: device, zeroSeconds: zeroSeconds)
            }
        )
        self.engine = engine
        return engine
    }

    /// Waits for everything already handed to the engine queue, which is where all of its work
    /// happens.
    private func drain(_ engine: Engine) {
        engine.queue.sync {}
    }

    private func runningDetectorUIDs(_ engine: Engine) -> Set<String> {
        engine.queue.sync { Set(engine.livenessDetectors.keys) }
    }

    private func status() -> Status? {
        StatusStore.read(from: statusURL)
    }

    private func logLines() -> [String] {
        EventLog.tail(1_000, url: logURL)
    }
}

// MARK: - Doubles

/// The audio system as a value. Writes land back in the snapshot, so a second reconcile finds the
/// state already held - the same reason the real engine converges instead of writing every beat.
final class FakeAudioSystem: AudioSystem, @unchecked Sendable {

    var snapshotValue: DeviceSnapshot
    var sampleRate: Double? = 48_000
    private(set) var writes: [String] = []

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
        snapshotValue.defaultOutput = device
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
        ListenerToken(
            object: device,
            address: AudioProperty.address(selector, scope: scope, element: element),
            queue: queue,
            block: { _, _ in }
        )
    }

    func removeListener(_ token: ListenerToken) {}
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
