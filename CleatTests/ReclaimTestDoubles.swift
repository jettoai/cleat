import CoreAudio
import Foundation
@testable import Cleat

// The reclaim engine harness, shared by ReclaimTests and ReclaimPresenceTests.

/// An engine with every outside edge replaced: the audio system, the routing daemon, the
/// pairing list, the clock and all three files.
final class ReclaimWorld {

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

    /// Who is at the Mac, as the snapshot would read it.
    func user(idle: TimeInterval?, displayHeldAwake: Bool = false) {
        system.snapshotValue.inputIdle = idle
        system.snapshotValue.displayHeldAwake = displayHeldAwake
    }
}

/// The routing daemon as one canned answer, delivered synchronously on the caller's queue -
/// which is the engine queue, exactly where the real client delivers it.
final class FakeRouting: RouteRequesting, @unchecked Sendable {

    let isAvailable: Bool
    /// nil means the request is never answered, which is how a lost reply is tested.
    var answer: RouteResponse?
    private(set) var addresses: [String] = []
    private(set) var scores: [Int32] = []
    private(set) var reasons: [String] = []
    /// The last request left unanswered, so a test can answer it late.
    private(set) var unanswered: (@Sendable (RouteResponse) -> Void)?

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
        guard let answer else {
            unanswered = completion
            return
        }
        completion(answer)
    }
}

final class FakePairings: BluetoothInventory, @unchecked Sendable {

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
