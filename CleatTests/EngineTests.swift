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
        let engine = try makeEngine(config: pinningWithLiveness, system: system, detectors: detectors)

        engine.start(microphone: .granted)
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
        let engine = try makeEngine(config: pinningWithLiveness, system: system, detectors: detectors)

        engine.start(microphone: .granted)
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

    /// An unanswered dialog must not hand the input to a device nobody has listened to yet. The
    /// receiver may be switched off, and taking the input now means giving it back three seconds
    /// after the answer arrives - the startup double switch the measuring gate exists to prevent.
    func testPendingPermissionDoesNotSwitchToUnmeasuredLivenessDevice() throws {
        let system = FakeAudioSystem(snapshot: DeviceSnapshot(
            devices: [Fixture.wireless, Fixture.brio], defaultInput: Fixture.brio.id
        ))
        let detectors = DetectorLog(startResults: [true])
        let engine = try makeEngine(config: pinningWithLiveness, system: system, detectors: detectors)

        engine.start(microphone: .pending)
        drain(engine)

        XCTAssertEqual(system.writes, [])
        XCTAssertEqual(detectors.startCount, 0)
        XCTAssertEqual(status()?.microphone, "not determined")
        XCTAssertEqual(status()?.liveness["Wireless microphone"], "awaiting microphone permission")

        // The answer arrives: now a detector is measuring for real, and the input stays put until
        // it has a verdict.
        engine.updateMicrophone(.granted)
        drain(engine)

        XCTAssertEqual(detectors.startCount, 1)
        XCTAssertEqual(runningDetectorUIDs(engine), [Fixture.wireless.uid])
        XCTAssertEqual(system.writes, [])
        XCTAssertEqual(status()?.liveness["Wireless microphone"], "measuring")
    }

    /// A refusal is not an unanswered dialog: it decides that liveness is off, and the device goes
    /// back to being untracked, which the input rule reads as present. Same setup as the pending
    /// test above, opposite answer, opposite outcome.
    func testDeniedPermissionTreatsALivenessDeviceAsPresent() throws {
        let system = FakeAudioSystem(snapshot: DeviceSnapshot(
            devices: [Fixture.wireless, Fixture.brio], defaultInput: Fixture.brio.id
        ))
        let detectors = DetectorLog(startResults: [true])
        let engine = try makeEngine(config: pinningWithLiveness, system: system, detectors: detectors)

        engine.start(microphone: .pending)
        drain(engine)
        XCTAssertEqual(system.writes, [])

        engine.updateMicrophone(.denied("denied"))
        drain(engine)

        XCTAssertEqual(system.writes, ["input:\(Fixture.wireless.id)"])
        XCTAssertEqual(detectors.startCount, 0)
        XCTAssertEqual(status()?.microphone, "denied")
        XCTAssertEqual(
            status()?.liveness["Wireless microphone"], "disabled (no microphone permission)"
        )
    }

    /// All four rules that need no microphone hold the desk while the dialog is still on screen:
    /// the input comes back off the blocked AirPods, the output moves to the preferred speakers,
    /// the input gain is pulled to target, and the balance is centred on the beat after the output
    /// settles. The detector follows the answer when it lands.
    func testAllFourRulesRunBeforeThePermissionAnswerAndDetectorsFollowIt() throws {
        var config = pinningWithLiveness
        config.output = ["Studio Display Speakers", "MacBook Pro Speakers"]
        config.balance = 0.5
        config.inputVolume = ["Brio 100": 75]

        let system = FakeAudioSystem(snapshot: DeviceSnapshot(
            devices: [
                Fixture.wireless, Fixture.brio, Fixture.airPods,
                Fixture.displaySpeakers, Fixture.macSpeakers
            ],
            defaultInput: Fixture.airPods.id,
            defaultOutput: Fixture.macSpeakers.id,
            outputBalance: 0.2,
            inputVolumes: [Fixture.brio.id: 0.6]
        ))
        let detectors = DetectorLog(startResults: [true])
        let engine = try makeEngine(config: config, system: system, detectors: detectors)

        engine.start(microphone: .pending)
        drain(engine)

        XCTAssertEqual(system.writes, [
            "input:\(Fixture.brio.id)",
            "output:\(Fixture.displaySpeakers.id)",
            "volume:\(Fixture.brio.id):0.75"
        ])
        // Balance stands aside on the pass that moves the default output - writing it then would
        // set the balance of the device being left - and lands on the next beat instead.
        engine.queue.sync { engine.reconcile() }
        XCTAssertEqual(system.writes, [
            "input:\(Fixture.brio.id)",
            "output:\(Fixture.displaySpeakers.id)",
            "volume:\(Fixture.brio.id):0.75",
            "balance:\(Fixture.displaySpeakers.id):0.50"
        ])

        XCTAssertEqual(detectors.startCount, 0)
        XCTAssertEqual(status()?.microphone, "not determined")
        XCTAssertEqual(status()?.liveness["Wireless microphone"], "awaiting microphone permission")

        let writesBeforeAnswer = system.writes
        engine.updateMicrophone(.granted)
        drain(engine)

        XCTAssertEqual(detectors.startCount, 1)
        XCTAssertEqual(runningDetectorUIDs(engine), [Fixture.wireless.uid])
        // Nothing was left to write: the locks were already held.
        XCTAssertEqual(system.writes, writesBeforeAnswer)
        XCTAssertEqual(status()?.microphone, "authorized")
        XCTAssertEqual(status()?.liveness["Wireless microphone"], "measuring")
        XCTAssertEqual(logLines().filter { $0.contains("microphone: authorized") }.count, 1)
    }

    /// An answer that says what the engine already knew is not worth a line in the log.
    func testUnchangedPermissionIsANoOp() throws {
        let system = FakeAudioSystem(snapshot: DeviceSnapshot(
            devices: [Fixture.wireless], defaultInput: Fixture.wireless.id
        ))
        let detectors = DetectorLog(startResults: [true])
        let engine = try makeEngine(config: pinningWithLiveness, system: system, detectors: detectors)

        engine.start(microphone: .granted)
        drain(engine)
        let linesBefore = logLines().count

        engine.updateMicrophone(.granted)
        drain(engine)

        XCTAssertEqual(logLines().count, linesBefore)
        XCTAssertEqual(detectors.startCount, 1)
        XCTAssertEqual(detectors.stopCount, 0)
    }

    // MARK: - Headphone arrivals

    /// The arrival is an edge, and this is the whole life of one: headphones already connected at
    /// launch are not an arrival, connecting them is, reconciling again on the same devices is not,
    /// and a hand-picked output afterwards stands.
    func testHeadphonesTakeOverOnArrivalOnlyAndLeaveAHandPickedOutputAlone() throws {
        let system = FakeAudioSystem(snapshot: DeviceSnapshot(
            devices: [Fixture.macStudioSpeakers, Fixture.airPods],
            defaultOutput: Fixture.macStudioSpeakers.id
        ))
        let engine = try makeEngine(
            config: headphonesConfig, system: system, detectors: DetectorLog(startResults: [true])
        )

        // Launch with the headphones already connected: Cleat restarting is not a reason to move
        // the output, so the first pass has no arrivals at all.
        engine.start(microphone: .granted)
        drain(engine)
        XCTAssertEqual(system.writes, [])

        // They disconnect. Still nothing to do: the speakers already hold the output.
        engine.queue.sync {
            system.snapshotValue.devices = [Fixture.macStudioSpeakers]
            engine.reconcile()
        }
        XCTAssertEqual(system.writes, [])

        // And they connect again. That is the arrival.
        engine.queue.sync {
            system.snapshotValue.devices = [Fixture.macStudioSpeakers, Fixture.airPods]
            engine.reconcile()
        }
        XCTAssertEqual(system.writes, ["output:\(Fixture.airPods.id)"])
        XCTAssertEqual(
            logLines().filter {
                $0.contains("pinOutput: Mac Studio的揚聲器 -> AirPods Max (headphones connected)")
            }.count,
            1
        )

        // The beats that follow a device change see the same devices, so the arrival is spent.
        engine.queue.sync { engine.reconcile() }
        engine.queue.sync { engine.reconcile() }
        XCTAssertEqual(system.writes, ["output:\(Fixture.airPods.id)"])

        // The user picks the speakers by hand with the headphones still on. Nothing takes them
        // back: the priority list is what plays when no headphones are around, and they are here.
        engine.queue.sync {
            system.snapshotValue.defaultOutput = Fixture.macStudioSpeakers.id
            engine.reconcile()
        }
        XCTAssertEqual(system.writes, ["output:\(Fixture.airPods.id)"])

        XCTAssertEqual(
            status()?.rules["headphones"], "on (bluetooth output takes over when it connects)"
        )
        XCTAssertEqual(
            status()?.rules["outputPin"],
            "on (Mac Studio的揚聲器), blocked: Maono AI Microphone"
        )
    }

    /// What happens after the headphones leave is the priority list's business again. macOS lands
    /// the output on the microphone's speaker end, which is on no priority list - only `blockedOutput`
    /// says it was not a choice.
    func testOutputListTakesOverOnceTheHeadphonesAreGone() throws {
        let maonoSpeakers = AudioDevice(
            id: 51, name: "Maono\u{00A0}AI Microphone", uid: "Maono-Output-UID",
            hasInput: false, hasOutput: true, transport: kAudioDeviceTransportTypeUSB
        )
        let system = FakeAudioSystem(snapshot: DeviceSnapshot(
            devices: [Fixture.macStudioSpeakers, maonoSpeakers, Fixture.airPods],
            defaultOutput: Fixture.airPods.id
        ))
        let engine = try makeEngine(
            config: headphonesConfig, system: system, detectors: DetectorLog(startResults: [true])
        )

        // The headphones hold the output and are not on the list: left alone.
        engine.start(microphone: .granted)
        drain(engine)
        XCTAssertEqual(system.writes, [])

        engine.queue.sync {
            system.snapshotValue.devices = [Fixture.macStudioSpeakers, maonoSpeakers]
            system.snapshotValue.defaultOutput = maonoSpeakers.id
            engine.reconcile()
        }
        XCTAssertEqual(system.writes, ["output:\(Fixture.macStudioSpeakers.id)"])
        XCTAssertEqual(
            logLines().filter {
                $0.contains("pinOutput: Maono\u{00A0}AI Microphone -> Mac Studio的揚聲器 (blocked)")
            }.count,
            1
        )
    }

    /// Which beats spend an arrival. The zero-delay beats the volume and balance listeners ask for
    /// can land while a device that has just appeared is not selectable yet, so they get to try
    /// without being the only attempt; the settle beat and everything after it consume.
    func testOnlyTheSettleBeatAndLaterConsumeArrivals() {
        XCTAssertFalse(Engine.consumesArrivals(after: 0))
        XCTAssertTrue(Engine.consumesArrivals(after: Engine.settleBeat))
        for beat in Engine.retryBeats {
            XCTAssertTrue(Engine.consumesArrivals(after: beat), "beat \(beat)")
        }
    }

    /// The write that reports success and does not stick, which is what a device that has just
    /// appeared does. The early beat tries and fails silently; because it did not spend the
    /// arrival, the settle beat sees the same one and tries again, this time for real.
    func testEarlyBeatThatDidNotStickLeavesTheArrivalForTheSettleBeat() throws {
        let system = FakeAudioSystem(snapshot: DeviceSnapshot(
            devices: [Fixture.macStudioSpeakers],
            defaultOutput: Fixture.macStudioSpeakers.id
        ))
        let engine = try makeEngine(
            config: headphonesConfig, system: system, detectors: DetectorLog(startResults: [true])
        )

        engine.start(microphone: .granted)
        drain(engine)
        XCTAssertEqual(system.writes, [])

        // The headphones connect. A volume or balance listener gets a zero-delay beat in first,
        // and the HAL is not ready to move the output yet.
        system.outputWritesStick = false
        engine.queue.sync {
            system.snapshotValue.devices = [Fixture.macStudioSpeakers, Fixture.airPods]
            engine.reconcile(consumingArrivals: false)
        }
        XCTAssertEqual(system.writes, ["output:\(Fixture.airPods.id)"])
        XCTAssertEqual(system.snapshotValue.defaultOutput, Fixture.macStudioSpeakers.id)

        // Half a second later the device list has settled and the same arrival is still there.
        system.outputWritesStick = true
        engine.queue.sync { engine.reconcile() }
        XCTAssertEqual(
            system.writes, ["output:\(Fixture.airPods.id)", "output:\(Fixture.airPods.id)"]
        )
        XCTAssertEqual(system.snapshotValue.defaultOutput, Fixture.airPods.id)

        // And now it is spent. The user picks the speakers by hand on the 1/3/6 beats, and no
        // beat takes them back.
        engine.queue.sync {
            system.snapshotValue.defaultOutput = Fixture.macStudioSpeakers.id
            engine.reconcile()
            engine.reconcile()
        }
        XCTAssertEqual(
            system.writes, ["output:\(Fixture.airPods.id)", "output:\(Fixture.airPods.id)"]
        )
    }

    /// The control: the early beat's write did stick, so the settle beat has nothing to do. Not
    /// spending the arrival early costs one extra pass over the devices, never a second switch.
    func testEarlyBeatThatStuckIsNotRepeatedOnTheSettleBeat() throws {
        let system = FakeAudioSystem(snapshot: DeviceSnapshot(
            devices: [Fixture.macStudioSpeakers],
            defaultOutput: Fixture.macStudioSpeakers.id
        ))
        let engine = try makeEngine(
            config: headphonesConfig, system: system, detectors: DetectorLog(startResults: [true])
        )

        engine.start(microphone: .granted)
        drain(engine)

        engine.queue.sync {
            system.snapshotValue.devices = [Fixture.macStudioSpeakers, Fixture.airPods]
            engine.reconcile(consumingArrivals: false)
        }
        XCTAssertEqual(system.writes, ["output:\(Fixture.airPods.id)"])
        XCTAssertEqual(system.snapshotValue.defaultOutput, Fixture.airPods.id)

        engine.queue.sync { engine.reconcile() }
        engine.queue.sync { engine.reconcile() }
        XCTAssertEqual(system.writes, ["output:\(Fixture.airPods.id)"])
        XCTAssertEqual(
            logLines().filter { $0.contains("(headphones connected)") }.count, 1
        )
    }

    // MARK: - Status

    /// With a wildcard the status line leads with the default and then names every input device
    /// present, by name, with the named entry that matches nothing here last. The gains are the
    /// ones read before this pass wrote anything.
    func testWildcardVolumeStatusLeadsWithTheDefault() throws {
        var config = pinningWithLiveness
        config.inputVolume = ["*": 100, "Brio 100": 75, "Wireless microphone": 88]

        let system = FakeAudioSystem(snapshot: DeviceSnapshot(
            devices: [Fixture.brio, Fixture.airPods],
            defaultInput: Fixture.brio.id,
            inputVolumes: [Fixture.brio.id: 0.75, Fixture.airPods.id: 0.97]
        ))
        let engine = try makeEngine(
            config: config, system: system, detectors: DetectorLog(startResults: [true])
        )

        engine.start(microphone: .granted)
        drain(engine)

        XCTAssertEqual(
            status()?.rules["inputVolume"],
            "on (default 100%; AirPods Max 100% (now 97%), Brio 100 75% (now 75%), "
                + "Wireless microphone 88% (absent))"
        )
        // The AirPods are held to the default even though they are a blocked input: the wildcard
        // covers every input device present.
        XCTAssertEqual(system.writes, ["volume:\(Fixture.airPods.id):1.00"])
    }

    /// Two devices sharing a name are both held to the override that names them, so the status
    /// says so twice rather than reporting the second one at the wildcard default.
    func testSameNamedDevicesBothReportTheOverride() throws {
        let secondBrio = AudioDevice(
            id: 21, name: "Brio 100", uid: "AppleUSBAudioEngine:Brio 100:C",
            hasInput: true, hasOutput: false
        )
        var config = pinningWithLiveness
        config.inputVolume = ["*": 100, "Brio 100": 75]

        let system = FakeAudioSystem(snapshot: DeviceSnapshot(
            devices: [Fixture.brio, secondBrio],
            defaultInput: Fixture.brio.id,
            inputVolumes: [Fixture.brio.id: 0.75, secondBrio.id: 0.60]
        ))
        let engine = try makeEngine(
            config: config, system: system, detectors: DetectorLog(startResults: [true])
        )

        engine.start(microphone: .granted)
        drain(engine)

        XCTAssertEqual(
            status()?.rules["inputVolume"],
            "on (default 100%; Brio 100 75% (now 75%), Brio 100 75% (now 60%))"
        )
        // And it is the override that is written, not the wildcard default.
        XCTAssertEqual(system.writes, ["volume:\(secondBrio.id):0.75"])
    }

    /// A device that is plugged in but whose gain could not be read this pass is unreadable, not
    /// absent: nothing is being held to the target, but the device is right here.
    func testPresentDeviceWithNoReadingIsUnreadableNotAbsent() throws {
        var config = pinningWithLiveness
        config.inputVolume = ["*": 100, "Brio 100": 75, "Wireless microphone": 88]

        let system = FakeAudioSystem(snapshot: DeviceSnapshot(
            devices: [Fixture.brio, Fixture.airPods],
            defaultInput: Fixture.brio.id,
            inputVolumes: [Fixture.airPods.id: 0.97]
        ))
        let engine = try makeEngine(
            config: config, system: system, detectors: DetectorLog(startResults: [true])
        )

        engine.start(microphone: .granted)
        drain(engine)

        XCTAssertEqual(
            status()?.rules["inputVolume"],
            "on (default 100%; AirPods Max 100% (now 97%), Brio 100 75% (unreadable), "
                + "Wireless microphone 88% (absent))"
        )
        // Unreadable means untouched: only the AirPods are written.
        XCTAssertEqual(system.writes, ["volume:\(Fixture.airPods.id):1.00"])
    }

    /// Without a wildcard the line is exactly what it always was: the configured entries, in key
    /// order, absent ones in place rather than at the end.
    func testNamedOnlyVolumeStatusIsUnchanged() throws {
        var config = pinningWithLiveness
        config.inputVolume = ["Brio 100": 75, "Wireless microphone": 88]

        let system = FakeAudioSystem(snapshot: DeviceSnapshot(
            devices: [Fixture.brio, Fixture.airPods],
            defaultInput: Fixture.brio.id,
            inputVolumes: [Fixture.brio.id: 0.75, Fixture.airPods.id: 0.97]
        ))
        let engine = try makeEngine(
            config: config, system: system, detectors: DetectorLog(startResults: [true])
        )

        engine.start(microphone: .granted)
        drain(engine)

        XCTAssertEqual(
            status()?.rules["inputVolume"],
            "on (Brio 100 75% (now 75%), Wireless microphone 88% (absent))"
        )
        // Nothing to write: the AirPods are not listed, so their gain is nobody's business.
        XCTAssertEqual(system.writes, [])
    }

    // MARK: - Helpers

    /// Albert's input pinning plus the one liveness entry, with the login item left out: the test
    /// host is a real app bundle, and `syncLaunchAtLogin` only stays out of SMAppService's way
    /// because the wanted state already matches.
    private var pinningWithLiveness: Config {
        Config(
            input: Fixture.pinnedInput.input,
            blockedInput: Fixture.pinnedInput.blockedInput,
            liveness: Fixture.pinnedInput.liveness,
            launchAtLogin: false
        )
    }

    /// Albert's output side: the speakers as the fallback, the microphone's speaker end blocked,
    /// and headphone takeover on. No input rules, so every write in these tests is an output one.
    private var headphonesConfig: Config {
        Config(
            output: ["Mac Studio的揚聲器"],
            blockedOutput: ["Maono AI Microphone"],
            headphonesTakeOver: true,
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
    /// CoreAudio answers `noErr` for a default-output write against a device that has just
    /// appeared and then quietly does not move the output. Setting this to false is that device.
    var outputWritesStick = true
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
