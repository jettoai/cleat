import CoreAudio
import XCTest
@testable import Cleat

/// One test per row of design.md 4.1.
final class InputPinRuleTests: XCTestCase {

    private func snapshot(
        _ devices: [AudioDevice],
        current: AudioDevice?,
        liveness: [String: Liveness] = [:]
    ) -> DeviceSnapshot {
        DeviceSnapshot(devices: devices, defaultInput: current?.id, liveness: liveness)
    }

    // Row 1: already on the top live device.
    func testCurrentIsTargetDoesNothing() {
        let snapshot = snapshot(
            [Fixture.wireless, Fixture.brio],
            current: Fixture.wireless,
            liveness: [Fixture.wireless.uid: .live]
        )
        XCTAssertEqual(InputPinRule.reconcile(snapshot, Fixture.pinnedInput), [])
    }

    // Row 2: a higher-priority mic was plugged in.
    func testHigherPriorityDeviceTakesOver() {
        let snapshot = snapshot(
            [Fixture.wireless, Fixture.brio],
            current: Fixture.brio,
            liveness: [Fixture.wireless.uid: .live]
        )
        XCTAssertEqual(
            InputPinRule.reconcile(snapshot, Fixture.pinnedInput),
            [.setDefaultInput(
                Fixture.wireless.id,
                reason: "Brio 100 -> Wireless microphone (higher priority present)"
            )]
        )
    }

    // Row 3: the receiver is still plugged in but the transmitter is off.
    func testSilentDeviceFallsBackToNextInList() {
        let snapshot = snapshot(
            [Fixture.wireless, Fixture.brio],
            current: Fixture.wireless,
            liveness: [Fixture.wireless.uid: .silent]
        )
        XCTAssertEqual(
            InputPinRule.reconcile(snapshot, Fixture.pinnedInput),
            [.setDefaultInput(
                Fixture.brio.id,
                reason: "Wireless microphone -> Brio 100 (Wireless microphone silent)"
            )]
        )
    }

    // Row 4: macOS grabbed the input when AirPods Max connected.
    func testBlockedDeviceIsReplaced() {
        let snapshot = snapshot([Fixture.airPods, Fixture.brio], current: Fixture.airPods)
        XCTAssertEqual(
            InputPinRule.reconcile(snapshot, Fixture.pinnedInput),
            [.setDefaultInput(Fixture.brio.id, reason: "AirPods Max -> Brio 100 (blocked)")]
        )
    }

    // Row 5: a conferencing app's virtual device is in use.
    func testVirtualConferencingDeviceIsLeftAlone() {
        let snapshot = snapshot(
            [Fixture.zoom, Fixture.wireless, Fixture.brio],
            current: Fixture.zoom,
            liveness: [Fixture.wireless.uid: .live]
        )
        XCTAssertEqual(InputPinRule.reconcile(snapshot, Fixture.pinnedInput), [])
    }

    // Row 6: a device the user picked by hand.
    func testHandPickedDeviceIsLeftAlone() {
        let snapshot = snapshot(
            [Fixture.maono, Fixture.wireless, Fixture.brio],
            current: Fixture.maono,
            liveness: [Fixture.wireless.uid: .live]
        )
        XCTAssertEqual(InputPinRule.reconcile(snapshot, Fixture.pinnedInput), [])
    }

    // Row 7: the fallback is already the current device.
    func testSilentTopDeviceButAlreadyOnFallback() {
        let snapshot = snapshot(
            [Fixture.wireless, Fixture.brio],
            current: Fixture.brio,
            liveness: [Fixture.wireless.uid: .silent]
        )
        XCTAssertEqual(InputPinRule.reconcile(snapshot, Fixture.pinnedInput), [])
    }

    // Row 8: nothing on the list is plugged in - do not force a choice.
    func testNoCandidateLeavesBlockedDeviceInPlace() {
        let snapshot = snapshot([Fixture.airPods], current: Fixture.airPods)
        XCTAssertEqual(InputPinRule.reconcile(snapshot, Fixture.pinnedInput), [])
    }

    // Row 9: the top device is plugged in but has no verdict yet - do not move to it.
    func testMeasuringDeviceIsNotASwitchTarget() {
        let snapshot = snapshot(
            [Fixture.wireless, Fixture.brio],
            current: Fixture.brio,
            liveness: [Fixture.wireless.uid: .measuring]
        )
        XCTAssertEqual(InputPinRule.reconcile(snapshot, Fixture.pinnedInput), [])
    }

    // Row 10: measuring only blocks arrivals, not incumbents - the device already holding the
    // default input keeps it while its first verdict is on the way.
    func testMeasuringIncumbentIsNotEvicted() {
        let snapshot = snapshot(
            [Fixture.wireless, Fixture.brio],
            current: Fixture.wireless,
            liveness: [Fixture.wireless.uid: .measuring]
        )
        XCTAssertEqual(InputPinRule.reconcile(snapshot, Fixture.pinnedInput), [])
    }

    func testEmptyPriorityListDisablesTheRule() {
        let snapshot = snapshot([Fixture.airPods, Fixture.brio], current: Fixture.airPods)
        XCTAssertEqual(InputPinRule.reconcile(snapshot, Config()), [])
    }

    func testUntrackedLivenessCountsAsPresent() {
        // No liveness key at all (nothing configured, or no microphone permission) is not the same
        // as .measuring: the gate is simply not in play, so behaviour is as it was before it existed.
        let snapshot = snapshot([Fixture.wireless, Fixture.brio], current: Fixture.brio)
        XCTAssertEqual(
            InputPinRule.reconcile(snapshot, Fixture.pinnedInput),
            [.setDefaultInput(
                Fixture.wireless.id,
                reason: "Brio 100 -> Wireless microphone (higher priority present)"
            )]
        )
    }

    func testOutputOnlyDeviceIsNotAnInputCandidate() {
        let config = Config(input: ["Studio Display Speakers", "Brio 100"], blockedInput: ["AirPods Max"])
        let snapshot = snapshot([Fixture.displaySpeakers, Fixture.airPods, Fixture.brio], current: Fixture.airPods)
        XCTAssertEqual(
            InputPinRule.reconcile(snapshot, config),
            [.setDefaultInput(Fixture.brio.id, reason: "AirPods Max -> Brio 100 (blocked)")]
        )
    }

    func testUIDEntryMatchesTheDevice() {
        let config = Config(input: [Fixture.wireless.uid, "Brio 100"], blockedInput: ["AirPods Max"])
        let snapshot = snapshot([Fixture.wireless, Fixture.brio], current: Fixture.brio)
        XCTAssertEqual(
            InputPinRule.reconcile(snapshot, config),
            [.setDefaultInput(
                Fixture.wireless.id,
                reason: "Brio 100 -> Wireless microphone (higher priority present)"
            )]
        )
    }

    func testNoDefaultInputDoesNothing() {
        let snapshot = snapshot([Fixture.wireless, Fixture.brio], current: nil)
        XCTAssertEqual(InputPinRule.reconcile(snapshot, Fixture.pinnedInput), [])
    }
}
