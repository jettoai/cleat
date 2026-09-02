import CoreAudio
import XCTest
@testable import Cleat

final class BalanceRuleTests: XCTestCase {

    private let config = Config(balance: 0.5)

    private func snapshot(balance: Float?, output: AudioDeviceID? = Fixture.airPods.id) -> DeviceSnapshot {
        DeviceSnapshot(devices: [Fixture.airPods], defaultOutput: output, outputBalance: balance)
    }

    func testWithinToleranceDoesNothing() {
        XCTAssertEqual(BalanceRule.reconcile(snapshot(balance: 0.49), config), [])
    }

    func testDriftIsPulledBack() {
        XCTAssertEqual(
            BalanceRule.reconcile(snapshot(balance: 0.3), config),
            [.setBalance(Fixture.airPods.id, 0.5, reason: "AirPods Max 0.30 -> 0.50")]
        )
    }

    func testUnreadableBalanceDoesNothing() {
        // nil means "the device is not ready yet", not "centred". Acting on it would write 0.5
        // into a device that is about to report its real value.
        XCTAssertEqual(BalanceRule.reconcile(snapshot(balance: nil), config), [])
    }

    func testDisabledConfigDoesNothing() {
        XCTAssertEqual(BalanceRule.reconcile(snapshot(balance: 0.3), Config()), [])
    }

    func testNoDefaultOutputDoesNothing() {
        XCTAssertEqual(BalanceRule.reconcile(snapshot(balance: 0.3, output: nil), config), [])
    }

    func testNonCenterTargetIsHonoured() {
        XCTAssertEqual(
            BalanceRule.reconcile(snapshot(balance: 0.5), Config(balance: 0.2)),
            [.setBalance(Fixture.airPods.id, 0.2, reason: "AirPods Max 0.50 -> 0.20")]
        )
    }
}
