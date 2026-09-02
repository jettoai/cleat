import CoreAudio
import XCTest
@testable import Cleat

final class OutputPinRuleTests: XCTestCase {

    private let config = Config(output: ["Studio Display Speakers", "MacBook Pro Speakers"])

    private func snapshot(_ devices: [AudioDevice], current: AudioDevice?) -> DeviceSnapshot {
        DeviceSnapshot(devices: devices, defaultOutput: current?.id)
    }

    func testAlreadyOnTargetDoesNothing() {
        let snapshot = snapshot([Fixture.displaySpeakers, Fixture.macSpeakers], current: Fixture.displaySpeakers)
        XCTAssertEqual(OutputPinRule.reconcile(snapshot, config), [])
    }

    func testHigherPriorityDeviceTakesOver() {
        let snapshot = snapshot([Fixture.displaySpeakers, Fixture.macSpeakers], current: Fixture.macSpeakers)
        XCTAssertEqual(
            OutputPinRule.reconcile(snapshot, config),
            [.setDefaultOutput(
                Fixture.displaySpeakers.id,
                reason: "MacBook Pro Speakers -> Studio Display Speakers (higher priority present)"
            )]
        )
    }

    func testDeviceOutsideTheListIsLeftAlone() {
        // AirPods Max is a hand-picked output here, not on the list: leave it.
        let snapshot = snapshot([Fixture.airPods, Fixture.displaySpeakers], current: Fixture.airPods)
        XCTAssertEqual(OutputPinRule.reconcile(snapshot, config), [])
    }

    func testEmptyListDisablesTheRule() {
        let snapshot = snapshot([Fixture.displaySpeakers, Fixture.macSpeakers], current: Fixture.macSpeakers)
        XCTAssertEqual(OutputPinRule.reconcile(snapshot, Config()), [])
    }

    func testNoListedDevicePresentDoesNothing() {
        let snapshot = snapshot([Fixture.airPods], current: Fixture.airPods)
        XCTAssertEqual(OutputPinRule.reconcile(snapshot, config), [])
    }

    func testInputOnlyDeviceIsNotAnOutputCandidate() {
        let config = Config(output: ["Brio 100", "MacBook Pro Speakers"])
        let snapshot = snapshot([Fixture.brio, Fixture.macSpeakers, Fixture.displaySpeakers], current: Fixture.displaySpeakers)
        // Brio has no output side, so the target is MacBook Pro Speakers - but the current output
        // is not on the list, so nothing happens.
        XCTAssertEqual(OutputPinRule.reconcile(snapshot, config), [])
    }

    func testNoDefaultOutputDoesNothing() {
        let snapshot = snapshot([Fixture.displaySpeakers, Fixture.macSpeakers], current: nil)
        XCTAssertEqual(OutputPinRule.reconcile(snapshot, config), [])
    }
}
