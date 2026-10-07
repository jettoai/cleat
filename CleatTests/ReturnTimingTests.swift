import CoreAudio
import XCTest
@testable import Cleat

/// How long a listed headset takes to come back (B-1173), with the lines cleat-rs writes:
/// Cleat's own accepted hijack, and macOS switching the headset back by itself.
final class ReturnTimingTests: XCTestCase {

    private let config = Config(reclaim: ["AirPods Max"])
    private let hijacked = RouteResponse(action: 1, reason: "Tipi device hijack was successful")

    private func world(answer: RouteResponse?) throws -> ReclaimWorld {
        let world = try ReclaimWorld(config: config)
        world.routing.answer = answer
        return world
    }

    /// Nobody at the Mac: playing on the speakers, nothing asked.
    private func nobodyPlaying() throws -> ReclaimWorld {
        let world = try world(answer: nil)
        world.user(idle: 120)
        return world
    }

    private func arrive(_ world: ReclaimWorld) {
        world.system.snapshotValue.devices = [Fixture.macStudioSpeakers, Fixture.airPods]
        world.system.snapshotValue.defaultOutput = Fixture.airPods.id
        world.reconcile()
    }

    private func count(_ world: ReclaimWorld, _ text: String) -> Int {
        world.lines { $0.contains(text) }
    }

    // MARK: - Cleat's own request

    func testAcceptedHijackLogsTheSecondsToTheOutput() throws {
        let world = try world(answer: hijacked)
        world.start()
        world.advance(1.8)
        arrive(world)
        XCTAssertEqual(count(world, "reclaim: AirPods Max back on the Mac in 1.8 s"), 1)
        world.advance(20)
        world.reconcile()
        XCTAssertEqual(count(world, "back on the Mac"), 1)
        XCTAssertEqual(count(world, "not back"), 0)
        XCTAssertEqual(count(world, "(macOS)"), 0)
    }

    func testHeadsetArrivingBeforeTheAnswerIsTimedFromTheRequest() throws {
        let world = try world(answer: nil)
        world.start()
        world.advance(0.4)
        arrive(world)
        XCTAssertEqual(count(world, "back on the Mac"), 0, "not logged before the hijack is accepted")
        world.engine.queue.sync { world.routing.unanswered?(hijacked) }
        XCTAssertEqual(count(world, "reclaim: AirPods Max back on the Mac in 0.4 s"), 1)
        XCTAssertEqual(count(world, "(macOS)"), 0)
    }

    func testAcceptedHijackThatNeverArrivesSaysSoAtTenSeconds() throws {
        let world = try world(answer: hijacked)
        world.start()
        world.advance(9.9)
        world.reconcile()
        XCTAssertEqual(count(world, "not back"), 0)
        world.advance(0.2)
        world.reconcile()
        XCTAssertEqual(
            count(world, "reclaim: AirPods Max not back on the Mac 10 s after the request (not in CoreAudio)"), 1
        )
    }

    func testRefusedRequestIsNotTimed() throws {
        // Answered after the pass, as the real client does.
        let world = try world(answer: nil)
        world.start()
        world.engine.queue.sync { world.routing.unanswered?(RouteResponse(action: 0, reason: "Out of ear")) }
        XCTAssertEqual(count(world, "refused (Out of ear)"), 1)
        world.advance(0.5)
        arrive(world)
        world.advance(20)
        world.reconcile()
        XCTAssertEqual(count(world, "back on the Mac"), 0)
    }

    // MARK: - macOS switching back by itself

    func testMacosSwitchingBackByItselfLogsOneLineWithTheSeconds() throws {
        let world = try nobodyPlaying()
        world.start()
        XCTAssertEqual(world.routing.addresses, [])
        world.advance(1.5)
        arrive(world)
        world.advance(20)
        world.reconcile()
        XCTAssertEqual(count(world, "reclaim: AirPods Max back on the Mac (macOS) in 1.5 s"), 1)
        XCTAssertEqual(count(world, "back on the Mac"), 1)
    }

    func testMacosSwitchSlowerThanTheLimitIsNotLogged() throws {
        for (after, lines) in [(31.0, 0), (29.0, 1)] {
            let world = try nobodyPlaying()
            world.start()
            world.advance(after)
            arrive(world)
            XCTAssertEqual(count(world, "(macOS)"), lines, "\(after) s")
        }
    }

    func testPlaybackStoppingVoidsTheClock() throws {
        let world = try nobodyPlaying()
        world.start()
        world.advance(5)
        world.system.snapshotValue.outputRunning = false
        world.reconcile()
        world.advance(5)
        world.system.snapshotValue.outputRunning = true
        world.reconcile()
        world.advance(1)
        arrive(world)
        XCTAssertEqual(count(world, "reclaim: AirPods Max back on the Mac (macOS) in 1.0 s"), 1, "timed from the new playback")

        let stopped = try nobodyPlaying()
        stopped.start()
        stopped.system.snapshotValue.outputRunning = false
        stopped.reconcile()
        arrive(stopped)
        XCTAssertEqual(count(stopped, "back on the Mac"), 0, "arrived after the playback stopped")
    }

    func testHeadsetMovedAwayByHandIsNotTimed() throws {
        let world = try nobodyPlaying()
        world.system.snapshotValue.devices = [Fixture.airPods, Fixture.wiredHeadphones]
        world.system.snapshotValue.defaultOutput = Fixture.airPods.id
        world.start()
        world.user(idle: 1)
        world.system.snapshotValue.defaultOutput = Fixture.wiredHeadphones.id
        world.reconcile()
        world.advance(2)
        world.system.snapshotValue.defaultOutput = Fixture.airPods.id
        world.reconcile()
        XCTAssertEqual(count(world, "back on the Mac"), 0)
    }

    func testHeadsetAlreadyTheOutputWhenPlaybackStartsIsNotTimed() throws {
        let world = try nobodyPlaying()
        world.system.snapshotValue.devices = [Fixture.airPods, Fixture.wiredHeadphones]
        world.system.snapshotValue.defaultOutput = Fixture.airPods.id
        world.system.snapshotValue.outputRunning = false
        world.start()
        world.system.snapshotValue.outputRunning = true
        world.reconcile()
        world.reconcile()
        XCTAssertEqual(count(world, "back on the Mac"), 0)
    }
}
