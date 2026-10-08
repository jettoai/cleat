import CoreAudio
import XCTest
@testable import Cleat

/// Rule 7 following the device the user is on (B-1173): asked back when someone is at the Mac,
/// left with the phone when nobody is, and never against an output the user picked by hand.
final class ReclaimPresenceTests: XCTestCase {

    private static let airPodsAddress = "70:F9:4A:B6:0C:C9"
    private let config = Config(reclaim: ["AirPods Max"])

    /// Playing on the AirPods, with the wired headphones as the other output in CoreAudio.
    private func onTheHeadset(config: Config? = nil) throws -> ReclaimWorld {
        let world = try ReclaimWorld(config: config ?? self.config)
        world.system.snapshotValue.devices = [Fixture.airPods, Fixture.wiredHeadphones]
        world.system.snapshotValue.defaultOutput = Fixture.airPods.id
        return world
    }

    private func moveOutput(_ world: ReclaimWorld, to device: AudioDevice) {
        world.system.snapshotValue.defaultOutput = device.id
    }

    // MARK: - The phone takes a headset that stays in CoreAudio

    /// 2026-10-07 17:23-17:36: the phone took the AirPods, they stayed in CoreAudio and the output
    /// fell to the wired headphones. Nobody at the Mac: left with the phone. Back at the keyboard:
    /// asked for at once, at the usual score.
    func testPhoneTakingAHeadsetThatStaysListedIsAskedBackWhenTheUserReturns() throws {
        let world = try onTheHeadset()
        world.start()

        world.user(idle: 40)
        moveOutput(world, to: Fixture.wiredHeadphones)
        world.reconcile()
        XCTAssertEqual(world.routing.addresses, [])

        world.user(idle: 2)
        world.reconcile()
        XCTAssertEqual(world.routing.addresses, [Self.airPodsAddress])
        XCTAssertEqual(world.routing.scores, [Engine.reclaimScore])
    }

    /// Coming back to the keyboard is no CoreAudio event, so waiting for the user is a beat of its own.
    func testIdleUserIsWaitedForOnItsOwnBeat() throws {
        let world = try onTheHeadset()
        world.start()

        world.user(idle: 40)
        moveOutput(world, to: Fixture.wiredHeadphones)
        world.reconcile()
        XCTAssertNotNil(world.engine.queue.sync { world.engine.pendingReconciles[Engine.reclaimRetryDelay] })
    }

    // MARK: - The user picks another output

    /// A switch made at the keyboard stands for the playback, well past any backoff, and says so once.
    func testHandPickedOutputStandsForThePlayback() throws {
        let world = try onTheHeadset()
        world.start()

        world.user(idle: 1)
        moveOutput(world, to: Fixture.wiredHeadphones)
        for _ in 0..<10 {
            world.advance(Engine.reclaimBackoff + 1)
            world.reconcile()
        }
        XCTAssertEqual(world.routing.addresses, [])
        XCTAssertEqual(world.lines { $0.contains("not asked (the user picked 外接耳機)") }, 1)
    }

    /// Cleat's own pin moving the output off the headset is not the user's choice.
    func testCleatsOwnPinIsNotTheUsersChoice() throws {
        let world = try onTheHeadset(config: Config(output: ["外接耳機", "AirPods Max"], reclaim: ["AirPods Max"]))
        world.user(idle: 1)
        world.start()
        world.reconcile()

        XCTAssertTrue(world.system.writes.contains("output:\(Fixture.wiredHeadphones.id)"))
        XCTAssertEqual(world.routing.addresses, [Self.airPodsAddress])
    }

    /// The choice ends when the headset is the output again; the next time it leaves is judged afresh.
    func testHandPickedChoiceEndsWhenTheHeadsetIsTheOutputAgain() throws {
        let world = try onTheHeadset()
        world.start()
        world.user(idle: 1)
        moveOutput(world, to: Fixture.wiredHeadphones)
        world.reconcile()

        moveOutput(world, to: Fixture.airPods)
        world.reconcile()
        world.user(idle: 40)
        moveOutput(world, to: Fixture.wiredHeadphones)
        world.reconcile()
        world.user(idle: 2)
        world.reconcile()
        XCTAssertEqual(world.routing.addresses, [Self.airPodsAddress])
    }

    // MARK: - Whether anyone is at the Mac

    func testIdleUserIsNotAskedForAndSaysSoOnce() throws {
        let world = try ReclaimWorld(config: config)
        world.user(idle: 120)
        world.start()
        for _ in 0..<5 {
            world.advance(Engine.reclaimRetryDelay)
            world.reconcile()
        }
        XCTAssertEqual(world.routing.addresses, [])
        XCTAssertEqual(world.lines { $0.contains("not asked (nobody is using the Mac)") }, 1)

        world.user(idle: 3)
        world.reconcile()
        XCTAssertEqual(world.routing.addresses.count, 1)
    }

    /// Back from the phone to watch a video on the Mac without touching anything: the player
    /// holding the display awake is someone being there.
    func testVideoKeepingTheDisplayAwakeCountsAsUsingTheMac() throws {
        let world = try ReclaimWorld(config: config)
        world.user(idle: 120, displayHeldAwake: true)
        world.start()
        XCTAssertEqual(world.routing.addresses, [Self.airPodsAddress])
        XCTAssertEqual(world.lines { $0.contains("nobody is using") }, 0)
    }

    func testUnreadableActivityCountsAsSomeoneThere() throws {
        let world = try ReclaimWorld(config: config)
        world.user(idle: nil)
        world.start()
        XCTAssertEqual(world.routing.addresses, [Self.airPodsAddress])
    }
}
