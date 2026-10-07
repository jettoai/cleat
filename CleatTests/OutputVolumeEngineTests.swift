import AudioToolbox
import CoreAudio
import XCTest
@testable import Cleat

/// The output volume hold inside a real engine on its real queue: the writer source and the
/// audio system are fakes, the clock and the scheduling are not.
final class OutputVolumeEngineTests: XCTestCase {

    private static let parallelsPath =
        "/Applications/Parallels Desktop.app/Contents/MacOS/Parallels VM.app/Contents/MacOS/prl_vm_app"
    private static let controlCenterPath = "/System/Library/CoreServices/ControlCenter.app/Contents/MacOS/ControlCenter"

    private var directory: URL!
    private var configURL: URL!
    private var statusURL: URL!
    private var logURL: URL!
    private var engine: Engine!

    override func setUpWithError() throws {
        directory = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("cleat-outvol-tests-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        configURL = directory.appendingPathComponent("config.json")
        statusURL = directory.appendingPathComponent("status.json")
        logURL = directory.appendingPathComponent("cleat.log")
    }

    override func tearDownWithError() throws {
        engine = nil
        try? FileManager.default.removeItem(at: directory)
    }

    // MARK: - Tests

    func testListedAppWriteIsRevertedAndReported() throws {
        let (system, engine, source) = try start(holdAgainst: ["Parallels Desktop"])

        engine.queue.async {
            source.emit(.write(Self.write(Self.parallelsPath, 0.5, 0.43)))
            system.snapshotValue.outputVolumes = [0.43, 0.43]
            system.fire(kAudioDevicePropertyVolumeScalar)
        }
        waitOnQueue(engine, seconds: 0.6)

        XCTAssertEqual(system.writes, ["outvol:30:0.5000"])
        let log = logLines()
        XCTAssertTrue(log.contains { $0.hasSuffix("outputVolume: AirPods Max 50% -> 43% writer=Parallels Desktop reverted") })
        XCTAssertTrue(log.contains { $0.hasSuffix("outputVolume: AirPods Max 43% -> 50% (reverted Parallels Desktop)") })
        let line = try XCTUnwrap(status()?.rules["outputVolume"])
        XCTAssertTrue(line.hasPrefix("on (holding against Parallels Desktop); last revert "), line)

        // status.json carries the block the settings screen reads, with exactly these two keys.
        let block = try XCTUnwrap(rawStatus()["outputVolume"] as? [String: String])
        XCTAssertEqual(Set(block.keys), ["state", "lastRevert"])
        XCTAssertEqual(block["state"], "on (holding against Parallels Desktop)")
        let revert = try XCTUnwrap(block["lastRevert"])
        XCTAssertNotNil(revert.range(of: #"^\d\d:\d\d 拉回 Parallels Desktop 改的音量 43% → 50%$"#, options: .regularExpression), revert)
    }

    func testExecutableNameEntryStillMatches() throws {
        let (system, engine, source) = try start(holdAgainst: ["prl_vm_app"])
        engine.queue.async {
            source.emit(.write(Self.write(Self.parallelsPath, 0.5, 0.43)))
            system.snapshotValue.outputVolumes = [0.43, 0.43]
            system.fire(kAudioDevicePropertyVolumeScalar)
        }
        waitOnQueue(engine, seconds: 0.6)

        XCTAssertEqual(system.writes, ["outvol:30:0.5000"])
        XCTAssertTrue(logLines().contains { $0.hasSuffix("(reverted prl_vm_app)") })
    }

    func testUnlistedWriterIsKeptAndLoggedOnce() throws {
        let (system, engine, source) = try start(holdAgainst: ["Parallels Desktop"])
        engine.queue.async {
            source.emit(.write(Self.write(Self.controlCenterPath, 0.5, 0.43, pid: 531)))
            system.snapshotValue.outputVolumes = [0.43, 0.43]
            system.fire(kAudioDevicePropertyVolumeScalar)
        }
        waitOnQueue(engine, seconds: 0.6)
        engine.queue.async { system.fire(kAudioDevicePropertyVolumeScalar) }
        waitOnQueue(engine, seconds: 0.5)

        XCTAssertEqual(system.writes, [])
        XCTAssertEqual(logLines().filter { $0.contains("writer=") },
                       logLines().filter { $0.hasSuffix("outputVolume: AirPods Max 50% -> 43% writer=ControlCenter kept") })
        XCTAssertEqual(logLines().filter { $0.contains("writer=") }.count, 1)
        let block = try XCTUnwrap(rawStatus()["outputVolume"] as? [String: String])
        XCTAssertEqual(block, ["state": "on (holding against Parallels Desktop)"])
    }

    /// Parallels set the microphone (control 261) 100% -> 50% while the output already sat at 50%:
    /// the output never moved, so nothing is written back.
    func testListedWriteToAnotherControlLeavesAnUnmovedOutputAlone() throws {
        let (system, engine, source) = try start(holdAgainst: ["Parallels Desktop"])
        engine.queue.async {
            source.emit(.write(Self.write(Self.parallelsPath, 1.0, 0.5, control: 261)))
        }
        waitOnQueue(engine, seconds: 0.6)
        XCTAssertEqual(system.writes, [])
    }

    /// A line the parser cannot read arrives while a listed write waits: the source is not
    /// trusted, so the write is not undone.
    func testUnrecognisedLineDuringAPendingWriteStopsTheRevert() throws {
        let (system, engine, source) = try start(holdAgainst: ["Parallels Desktop"])
        engine.queue.async {
            source.emit(.write(Self.write(Self.parallelsPath, 0.5, 0.43)))
            system.snapshotValue.outputVolumes = [0.43, 0.43]
            system.fire(kAudioDevicePropertyVolumeScalar)
            source.emit(.unrecognised("volume changed by something new"))
        }
        waitOnQueue(engine, seconds: 0.6)
        XCTAssertEqual(system.writes, [])
        XCTAssertTrue(logLines().contains { $0.hasSuffix("writer=Parallels Desktop kept (paused)") })
    }

    func testSourceThatCannotStartPausesTheHold() throws {
        let (system, engine, _) = try start(holdAgainst: ["Parallels Desktop"], sourceStarts: false)
        XCTAssertEqual(status()?.rules["outputVolume"],
                       "paused: cannot read system log writer (log stream could not start)")

        engine.queue.async {
            system.snapshotValue.outputVolumes = [0.43, 0.43]
            system.fire(kAudioDevicePropertyVolumeScalar)
        }
        waitOnQueue(engine, seconds: 0.6)
        XCTAssertEqual(system.writes, [])
    }

    func testSourceThatExitsIsRestarted() throws {
        let (_, engine, source) = try start(holdAgainst: ["Parallels Desktop"])
        XCTAssertEqual(source.startCount, 1)

        engine.queue.sync {
            source.exit(1)
            engine.reconcile()
        }
        XCTAssertEqual(status()?.rules["outputVolume"], "paused: cannot read system log writer (log stream exited (1))")
        XCTAssertTrue(logLines().contains { $0.hasSuffix("outputVolume: writer log stopped (exit 1), restarting in 1s") })

        waitOnQueue(engine, seconds: 1.2)
        XCTAssertEqual(source.startCount, 2)
        engine.queue.sync { engine.reconcile() }
        XCTAssertEqual(status()?.rules["outputVolume"], "on (holding against Parallels Desktop)")
    }

    func testEmptyListNeverStartsTheSource() throws {
        var made = 0
        let (system, engine, _) = try start(holdAgainst: [], onMake: { made += 1 })
        engine.queue.async {
            system.snapshotValue.outputVolumes = [0.43, 0.43]
            system.fire(kAudioDevicePropertyVolumeScalar)
        }
        waitOnQueue(engine, seconds: 0.5)

        XCTAssertEqual(made, 0)
        XCTAssertEqual(status()?.rules["outputVolume"], "off")
        XCTAssertEqual(system.writes, [])
    }

    /// Parallels writes one channel, then the other 240ms later: the balance listener sees a
    /// shift, the hold puts the volume back first, and the balance rule finds nothing to do.
    func testRevertAndBalanceDoNotFight() throws {
        let (system, engine, source) = try start(holdAgainst: ["Parallels Desktop"], balance: 0.5)

        engine.queue.async {
            source.emit(.write(Self.write(Self.parallelsPath, 0.5, 0.43)))
            system.snapshotValue.outputVolumes = [0.43, 0.5]
            system.snapshotValue.outputBalance = 0.46
            system.fire(kAudioDevicePropertyVolumeScalar)
            system.fire(kAudioHardwareServiceDeviceProperty_VirtualMainBalance)
        }
        engine.queue.asyncAfter(deadline: .now() + 0.24) {
            source.emit(.write(Self.write(Self.parallelsPath, 0.5, 0.43, control: 263)))
            system.snapshotValue.outputVolumes = [0.43, 0.43]
            system.snapshotValue.outputBalance = 0.5
            system.fire(kAudioDevicePropertyVolumeScalar)
            system.fire(kAudioHardwareServiceDeviceProperty_VirtualMainBalance)
        }
        waitOnQueue(engine, seconds: 1)

        XCTAssertEqual(system.writes, ["outvol:30:0.5000"])
    }

    // MARK: - Helpers

    private static func write(_ path: String, _ from: Float, _ to: Float,
                       pid: Int32 = 8484, control: Int = 262) -> VolumeWrite {
        VolumeWrite(at: Date(), pid: pid, writer: path, control: control, from: from, to: to)
    }

    private func start(
        holdAgainst: [String], balance: Double? = nil, sourceStarts: Bool = true, onMake: @escaping () -> Void = {}
    ) throws -> (FakeAudioSystem, Engine, FakeVolumeWriterSource) {
        let config = Config(balance: balance, launchAtLogin: false, outputVolumeHoldAgainst: holdAgainst)
        let system = FakeAudioSystem(snapshot: DeviceSnapshot(
            devices: [Fixture.airPods],
            defaultOutput: Fixture.airPods.id,
            outputBalance: 0.5,
            outputVolumes: [0.5, 0.5]
        ))
        try JSONEncoder().encode(config).write(to: configURL)
        let engine = Engine(
            system: system,
            log: EventLog(url: logURL, rotatedURL: directory.appendingPathComponent("cleat.log.1")),
            configURL: configURL,
            statusURL: statusURL,
            makeDetector: { device, _, zeroSeconds, _, _ in
                DetectorLog(startResults: [true]).make(device: device, zeroSeconds: zeroSeconds)
            }
        )
        self.engine = engine
        let source = FakeVolumeWriterSource()
        source.startResult = sourceStarts
        engine.queue.sync {
            engine.outputVolume.makeSource = {
                onMake()
                return source
            }
        }
        engine.start(microphone: .granted)
        engine.queue.sync {}
        XCTAssertEqual(system.writes, [])
        return (system, engine, source)
    }

    private func waitOnQueue(_ engine: Engine, seconds: TimeInterval) {
        let done = expectation(description: "engine queue after \(seconds)s")
        engine.queue.asyncAfter(deadline: .now() + seconds) { done.fulfill() }
        wait(for: [done], timeout: seconds + 2)
    }

    private func status() -> Status? {
        StatusStore.read(from: statusURL)
    }

    private func rawStatus() throws -> [String: Any] {
        let data = try Data(contentsOf: statusURL)
        return try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
    }

    private func logLines() -> [String] {
        EventLog.tail(1_000, url: logURL)
    }
}
