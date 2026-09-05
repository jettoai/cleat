import CoreAudio
import XCTest
@testable import Cleat

/// Rule 7, in three layers: the pure rule and its four conditions, the classification of what the
/// routing daemon answers, and the engine bookkeeping that decides when to ask again.
///
/// None of it touches the private framework or a radio: the rule takes a list of headsets as a
/// value, and both the routing client and the pairing list are protocols with doubles here.
final class ReclaimTests: XCTestCase {

    // MARK: - Fixtures

    private static let airPodsAddress = "70:F9:4A:B6:0C:C9"

    private let connectedAirPods = BluetoothHeadset(
        name: "AirPods Max", address: ReclaimTests.airPodsAddress, isConnected: true
    )
    private let connectedAirPodsPro = BluetoothHeadset(
        name: "AirPods Pro", address: "7C:F3:4D:68:72:76", isConnected: true
    )
    private let mouse = BluetoothHeadset(
        name: "MX Master 3S", address: "D0:70:FB:5A:99:FE", isConnected: true
    )

    private let config = Config(reclaim: ["AirPods Max"])

    /// The Mac is playing through its own speakers, and the headset is nowhere in the device list.
    /// That is the situation the whole rule is about.
    private func playing(_ devices: [AudioDevice] = [Fixture.macStudioSpeakers]) -> DeviceSnapshot {
        DeviceSnapshot(
            devices: devices,
            defaultOutput: devices.first?.id,
            outputRunning: true
        )
    }

    private func request(_ headset: BluetoothHeadset) -> Action {
        .requestRoute(
            name: headset.name,
            address: headset.address,
            reason: ReclaimRule.requestReason(for: headset)
        )
    }

    // MARK: - The rule

    func testAllFourConditionsAskForTheHeadset() {
        XCTAssertEqual(
            ReclaimRule.reconcile(playing(), [connectedAirPods], config),
            [request(connectedAirPods)]
        )
    }

    /// (a) The headset is not on the list. Somebody else's AirPods connecting to this Mac is not
    /// an invitation to take them off their phone.
    func testUnlistedHeadsetIsLeftAlone() {
        // A mouse is in the pairing list too, and every other Bluetooth thing on the desk: only
        // what the config names is ever asked for.
        XCTAssertEqual(
            ReclaimRule.reconcile(playing(), [connectedAirPodsPro, mouse], config), []
        )
    }

    /// (b) Listed, but not connected to this Mac. There is nothing here to ask about: the routing
    /// request goes to an accessory that is not on the other end of a link.
    func testDisconnectedHeadsetIsNotAskedFor() {
        let away = BluetoothHeadset(
            name: "AirPods Max", address: Self.airPodsAddress, isConnected: false
        )
        XCTAssertEqual(ReclaimRule.reconcile(playing(), [away], config), [])
    }

    /// (c) The headset is already a CoreAudio output device. Whether it holds the output is the
    /// output rules' business; this rule only fills the gap where there is no device at all.
    func testHeadsetThatIsAlreadyAnAudioDeviceIsNotAskedFor() {
        let snapshot = playing([Fixture.macStudioSpeakers, Fixture.airPods])
        XCTAssertEqual(ReclaimRule.reconcile(snapshot, [connectedAirPods], config), [])
    }

    /// The device list carries the headset, but only as an input. A headset half way through
    /// negotiating has no output to send anything to, so the gap is still open.
    func testHeadsetPresentAsInputOnlyIsStillAskedFor() {
        let inputSide = AudioDevice(
            id: 33, name: "AirPods Max", uid: "AirPodsMax-Input-UID",
            hasInput: true, hasOutput: false, transport: kAudioDeviceTransportTypeBluetooth
        )
        let snapshot = playing([Fixture.macStudioSpeakers, inputSide])
        XCTAssertEqual(
            ReclaimRule.reconcile(snapshot, [connectedAirPods], config),
            [request(connectedAirPods)]
        )
    }

    /// (d) The Mac is silent. Nothing is being taken from anybody until this machine has something
    /// to play, which is the difference between a rule and a daemon fighting a phone all day.
    func testIdleMacDoesNotAskForAnything() {
        var snapshot = playing()
        snapshot.outputRunning = false
        XCTAssertEqual(ReclaimRule.reconcile(snapshot, [connectedAirPods], config), [])
    }

    func testEmptyConfigTurnsTheRuleOff() {
        XCTAssertEqual(ReclaimRule.reconcile(playing(), [connectedAirPods], Config()), [])
    }

    /// Two listed headsets connected at once is a tie broken by name, the same way every other
    /// list in Cleat is ordered - and only one request goes out, because only one device can hold
    /// the output.
    func testTwoListedHeadsetsAskForOneOfThemByName() {
        var config = self.config
        config.reclaim = ["AirPods Max", "AirPods Pro"]
        XCTAssertEqual(
            ReclaimRule.reconcile(playing(), [connectedAirPodsPro, connectedAirPods], config),
            [request(connectedAirPods)]
        )
    }

    /// The engine's memory of what it asked for lately reaches the rule as an exclusion list, so
    /// the single request a pass allows is spent on a headset that can actually be asked for
    /// rather than on the first name in the list regardless.
    func testBlockedHeadsetStepsAsideForTheNextOne() {
        var config = self.config
        config.reclaim = ["AirPods Max", "AirPods Pro"]

        XCTAssertEqual(
            ReclaimRule.reconcile(
                playing(), [connectedAirPodsPro, connectedAirPods], config,
                excluding: [connectedAirPods.address]
            ),
            [request(connectedAirPodsPro)]
        )
        // Every candidate blocked is nothing asked for, not the next name down anyway.
        XCTAssertEqual(
            ReclaimRule.reconcile(
                playing(), [connectedAirPodsPro, connectedAirPods], config,
                excluding: [connectedAirPods.address, connectedAirPodsPro.address]
            ),
            []
        )
    }

    // MARK: - Naming a headset

    func testHeadsetIsNamedByAddressInEitherForm() {
        XCTAssertTrue(connectedAirPods.isListed(in: [Self.airPodsAddress]))
        // The form the Bluetooth pane shows, which is what someone would copy.
        XCTAssertTrue(connectedAirPods.isListed(in: ["70-f9-4a-b6-0c-c9"]))
        XCTAssertFalse(connectedAirPods.isListed(in: ["70:F9:4A:B6:0C:CA"]))
    }

    /// Names keep their case, exactly as `DeviceName` intends. Addresses do not: hex is hex.
    func testNameMatchingIsCaseSensitiveAndAddressMatchingIsNot() {
        XCTAssertFalse(connectedAirPods.isListed(in: ["airpods max"]))
        XCTAssertTrue(connectedAirPods.isListed(in: ["70:f9:4a:b6:0c:c9"]))
    }

    func testCanonicalAddressNormalisesWhatIOBluetoothReports() {
        XCTAssertEqual(
            BluetoothHeadset.canonicalAddress("70-f9-4a-b6-0c-c9"), Self.airPodsAddress
        )
        XCTAssertEqual(
            BluetoothHeadset.canonicalAddress(Self.airPodsAddress), Self.airPodsAddress
        )
    }

    // MARK: - Reading the answer

    /// The five answers seen on the wire, each mapping to the one thing the engine does about it.
    func testOutcomesAreReadFromTheResponse() {
        XCTAssertEqual(
            RouteResponse(action: 1, reason: "Tipi device hijack was successful").outcome,
            .routed
        )
        XCTAssertEqual(
            RouteResponse(action: 0, reason: "Device already routed").outcome,
            .alreadyRouted
        )
        // Reported as routed and yet nothing changed: it was here before we asked, and that is
        // what the engine must not write a line about.
        XCTAssertEqual(
            RouteResponse(action: 1, reason: "already routed, wxInfo NULL").outcome,
            .alreadyRouted
        )
        XCTAssertEqual(
            RouteResponse(
                action: 0, reason: "Rejected, Remote Category 301 > Local Category 200, audio streaming"
            ).outcome,
            .heldByRemote("Remote Category 301 > Local Category 200, audio streaming")
        )
        XCTAssertEqual(
            RouteResponse(action: 0, reason: "Previous hijack hasn't finished").outcome,
            .busy
        )
        XCTAssertEqual(
            RouteResponse(action: 0, reason: "Something new in a later macOS").outcome,
            .refused("Something new in a later macOS")
        )
        XCTAssertEqual(RouteResponse(action: 0, error: "code 5").outcome, .refused("code 5"))
        XCTAssertEqual(RouteResponse().outcome, .refused("no reason given"))
    }

    // MARK: - The engine

    /// The whole life of one reclaim: the request goes out once, the answer is logged once, and
    /// the beats that follow do not ask again while the throttle holds.
    func testAcceptedHijackIsLoggedOnceAndThenThrottled() throws {
        let world = try World(config: config)
        world.routing.answer = RouteResponse(action: 1, reason: "Tipi device hijack was successful")

        world.start()
        XCTAssertEqual(world.routing.addresses, [Self.airPodsAddress])
        XCTAssertEqual(world.lines { $0.contains("hijack accepted") }, 1)

        // The beats that follow a granted hijack, plus anything else that reconciles: no second
        // request until the interval is up.
        world.reconcile()
        world.reconcile()
        XCTAssertEqual(world.routing.addresses, [Self.airPodsAddress])

        world.advance(Engine.reclaimInterval + 1)
        world.reconcile()
        XCTAssertEqual(world.routing.addresses, [Self.airPodsAddress, Self.airPodsAddress])
    }

    /// The score is the whole policy, so it is pinned to a literal rather than to the constant:
    /// 201 is a playback session, which beats an idle phone (100) and loses to one playing media
    /// (301) or on a call (501). Raising it to 301 would win the headset off a phone that is
    /// playing and lose it again the moment that phone asks back.
    func testRequestCarriesTheHijackScoreAndReason() throws {
        let world = try World(config: config)
        world.routing.answer = RouteResponse(action: 1, reason: "Tipi device hijack was successful")

        world.start()

        XCTAssertEqual(world.routing.scores, [201])
        XCTAssertEqual(
            world.routing.reasons,
            [ReclaimRule.requestReason(for: connectedAirPods)]
        )
    }

    /// The phone is playing and wins. One line, not one per beat, and a full backoff before the
    /// next attempt - and when the phone finally lets go, the line is worth writing again.
    func testRemoteHoldIsLoggedOncePerSpellAndBacksOff() throws {
        let world = try World(config: config)
        world.routing.answer = RouteResponse(
            action: 0, reason: "Rejected, Remote Category 301 > Local Category 200, audio streaming"
        )

        world.start()
        XCTAssertEqual(world.routing.addresses.count, 1)
        XCTAssertEqual(
            world.lines { $0.contains("held by remote device (Remote Category 301 > Local Category 200") },
            1
        )

        // The throttle would be up, but the backoff is longer and still holds.
        world.advance(Engine.reclaimInterval + 1)
        world.reconcile()
        XCTAssertEqual(world.routing.addresses.count, 1)

        world.advance(Engine.reclaimBackoff)
        world.reconcile()
        XCTAssertEqual(world.routing.addresses.count, 2)
        // Still held, still one line: the log says what changed, and nothing did.
        XCTAssertEqual(world.lines { $0.contains("held by remote device") }, 1)

        // The call ends and the headset comes back. The next spell reports for itself.
        world.routing.answer = RouteResponse(action: 1, reason: "Tipi device hijack was successful")
        world.advance(Engine.reclaimBackoff + 1)
        world.reconcile()
        world.routing.answer = RouteResponse(
            action: 0, reason: "Rejected, Remote Category 501 > Local Category 201, phone call"
        )
        world.advance(Engine.reclaimInterval + 1)
        world.reconcile()
        XCTAssertEqual(world.lines { $0.contains("held by remote device") }, 2)
    }

    /// "It was already here" is the answer on every machine where nothing is wrong, so it must be
    /// silent: a line every time the Mac starts playing would be a log nobody reads.
    func testAlreadyRoutedIsNotLogged() throws {
        let world = try World(config: config)
        world.routing.answer = RouteResponse(action: 1, reason: "Device already routed")

        world.start()
        XCTAssertEqual(world.routing.addresses.count, 1)
        XCTAssertEqual(world.lines { $0.contains("reclaim:") }, 0)
    }

    /// A request of ours that is still running is not a reason to wait: the throttle is dropped so
    /// the next beat asks again, and nothing is written about it.
    func testBusyAnswerClearsTheThrottleAndSaysNothing() throws {
        let world = try World(config: config)
        world.routing.answer = RouteResponse(action: 0, reason: "Previous hijack hasn't finished")

        world.start()
        XCTAssertEqual(world.routing.addresses.count, 1)

        world.reconcile()
        XCTAssertEqual(world.routing.addresses.count, 2)
        XCTAssertEqual(world.lines { $0.contains("reclaim:") }, 0)
    }

    /// A request the daemon never answers is a request that must expire. The reply is an XPC
    /// message like any other; if it is lost, the headset was blocked until the next restart.
    func testUnansweredRequestStopsBlockingAfterTheInterval() throws {
        let world = try World(config: config)
        // No answer at all - the routing daemon takes the request and says nothing.
        world.routing.answer = nil

        world.start()
        XCTAssertEqual(world.routing.addresses, [Self.airPodsAddress])

        // Inside the interval nothing is asked again, exactly as when an answer did arrive.
        world.reconcile()
        XCTAssertEqual(world.routing.addresses, [Self.airPodsAddress])

        world.advance(Engine.reclaimInterval + 1)
        world.reconcile()
        XCTAssertEqual(world.routing.addresses, [Self.airPodsAddress, Self.airPodsAddress])
    }

    /// Two listed headsets, the first one held by a phone. The pass that cannot ask for it is the
    /// second one's turn: a backoff on the first headset used to swallow every beat, and the other
    /// headset was never asked for at all.
    func testBackedOffHeadsetDoesNotStarveTheOtherOne() throws {
        var config = self.config
        config.reclaim = ["AirPods Max", "AirPods Pro"]
        let world = try World(config: config, headsets: [connectedAirPodsPro, connectedAirPods])
        world.routing.answer = RouteResponse(
            action: 0, reason: "Rejected, Remote Category 301 > Local Category 200, audio streaming"
        )

        world.start()
        XCTAssertEqual(world.routing.addresses, [Self.airPodsAddress])

        world.reconcile()
        XCTAssertEqual(
            world.routing.addresses, [Self.airPodsAddress, connectedAirPodsPro.address]
        )
    }

    /// A macOS without the routing class turns the rule off: one line, and nothing is ever asked.
    /// The status line says so too, so `cleat status` does not read as "on" while it is inert.
    func testUnavailableRoutingIsReportedOnceAndNeverAsks() throws {
        let world = try World(config: config, routing: FakeRouting(available: false))

        world.start()
        world.reconcile()
        world.reconcile()

        XCTAssertEqual(world.routing.addresses, [])
        XCTAssertEqual(world.lines { $0.contains("reclaim: unavailable on this macOS") }, 1)
        XCTAssertEqual(
            world.status()?.rules["reclaim"],
            "unavailable (no routing service on this macOS) (AirPods Max)"
        )
    }

    /// With no `reclaim` list the rule costs nothing: the pairing list is not even read, and the
    /// status line says off rather than naming a device.
    func testRuleOffDoesNotTouchBluetooth() throws {
        let world = try World(config: Config(launchAtLogin: false))

        world.start()

        XCTAssertEqual(world.bluetooth.reads, 0)
        XCTAssertEqual(world.routing.addresses, [])
        XCTAssertEqual(world.status()?.rules["reclaim"], "off")
    }

    func testStatusNamesTheListedHeadsets() throws {
        var config = self.config
        config.reclaim = ["AirPods Max", "70:F9:4A:B6:0C:C9"]
        let world = try World(config: config)
        world.routing.answer = RouteResponse(action: 1, reason: "Device already routed")

        world.start()

        XCTAssertEqual(world.status()?.rules["reclaim"], "on (AirPods Max, 70:F9:4A:B6:0C:C9)")
    }

    // MARK: - Harness

    /// An engine with every outside edge replaced: the audio system, the routing daemon, the
    /// pairing list, the clock and all three files.
    private final class World {

        let routing: FakeRouting
        let bluetooth: FakePairings
        let system: FakeAudioSystem
        let engine: Engine

        private let directory: URL
        private let statusURL: URL
        private let logURL: URL
        private let clock = Clock()

        /// The engine reads the time through a closure, so the throttle and the backoff can be
        /// walked forward a minute at a time without the test taking a minute.
        final class Clock: @unchecked Sendable {
            var now = Date(timeIntervalSince1970: 1_700_000_000)
        }

        init(
            config: Config,
            routing: FakeRouting = FakeRouting(available: true),
            headsets: [BluetoothHeadset] = [
                BluetoothHeadset(name: "AirPods Max", address: "70:F9:4A:B6:0C:C9", isConnected: true)
            ]
        ) throws {
            directory = URL(fileURLWithPath: NSTemporaryDirectory())
                .appendingPathComponent("cleat-reclaim-tests-\(UUID().uuidString)", isDirectory: true)
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
            let configURL = directory.appendingPathComponent("config.json")
            statusURL = directory.appendingPathComponent("status.json")
            logURL = directory.appendingPathComponent("cleat.log")

            var config = config
            // The test host is a real app bundle; leaving this on would have the engine talk to
            // SMAppService about it.
            config.launchAtLogin = false
            try JSONEncoder().encode(config).write(to: configURL)

            self.routing = routing
            bluetooth = FakePairings(headsets: headsets)
            // Playing through the Mac's own speakers, with the headset absent from CoreAudio.
            system = FakeAudioSystem(snapshot: DeviceSnapshot(
                devices: [Fixture.macStudioSpeakers],
                defaultOutput: Fixture.macStudioSpeakers.id,
                outputRunning: true
            ))

            let clock = self.clock
            engine = Engine(
                system: system,
                log: EventLog(
                    url: logURL, rotatedURL: directory.appendingPathComponent("cleat.log.1")
                ),
                configURL: configURL,
                statusURL: statusURL,
                makeDetector: { device, _, zeroSeconds, _, _ in
                    DetectorLog(startResults: [true]).make(device: device, zeroSeconds: zeroSeconds)
                },
                routing: routing,
                bluetooth: bluetooth,
                now: { clock.now }
            )
        }

        deinit {
            try? FileManager.default.removeItem(at: directory)
        }

        func start() {
            engine.start(microphone: .denied("no microphone in tests"))
            engine.queue.sync {}
        }

        func reconcile() {
            engine.queue.sync { engine.reconcile() }
        }

        func advance(_ seconds: TimeInterval) {
            clock.now = clock.now.addingTimeInterval(seconds)
        }

        func lines(_ matching: (String) -> Bool) -> Int {
            EventLog.tail(1_000, url: logURL).filter(matching).count
        }

        func status() -> Status? {
            StatusStore.read(from: statusURL)
        }
    }

    /// The routing daemon as one canned answer, delivered synchronously on the caller's queue -
    /// which is the engine queue, exactly where the real client delivers it.
    private final class FakeRouting: RouteRequesting, @unchecked Sendable {

        let isAvailable: Bool
        /// nil means the request is never answered, which is how a lost reply is tested.
        var answer: RouteResponse?
        private(set) var addresses: [String] = []
        private(set) var scores: [Int32] = []
        private(set) var reasons: [String] = []

        init(available: Bool) {
            isAvailable = available
        }

        func request(
            address: String,
            score: Int32,
            reason: String,
            queue: DispatchQueue,
            completion: @escaping @Sendable (RouteResponse) -> Void
        ) {
            addresses.append(address)
            scores.append(score)
            reasons.append(reason)
            guard let answer else { return }
            completion(answer)
        }
    }

    private final class FakePairings: BluetoothInventory, @unchecked Sendable {

        var headsets: [BluetoothHeadset]
        private(set) var reads = 0

        init(headsets: [BluetoothHeadset]) {
            self.headsets = headsets
        }

        func pairedHeadsets() -> [BluetoothHeadset] {
            reads += 1
            return headsets
        }
    }
}
