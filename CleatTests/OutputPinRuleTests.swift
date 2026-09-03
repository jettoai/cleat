import CoreAudio
import XCTest
@testable import Cleat

final class OutputPinRuleTests: XCTestCase {

    private let config = Config(output: ["Studio Display Speakers", "MacBook Pro Speakers"])

    /// The speaker end of the USB microphone: an output nobody wants, which macOS falls back to
    /// when the current output leaves.
    private let maonoSpeakers = AudioDevice(
        id: 51, name: "Maono\u{00A0}AI Microphone", uid: "Maono-Output-UID",
        hasInput: false, hasOutput: true, transport: kAudioDeviceTransportTypeUSB
    )

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

    // MARK: - Blocked outputs

    /// A USB microphone's speaker end is where macOS lands when the headphones leave. It is not on
    /// the priority list, so only the blocked list tells this rule that it was not a choice.
    func testBlockedCurrentOutputIsReplacedByTheFirstListedDevice() {
        var config = self.config
        config.blockedOutput = ["Maono AI Microphone"]
        let snapshot = snapshot(
            [maonoSpeakers, Fixture.displaySpeakers, Fixture.macSpeakers], current: maonoSpeakers
        )
        XCTAssertEqual(
            OutputPinRule.reconcile(snapshot, config),
            [.setDefaultOutput(
                Fixture.displaySpeakers.id,
                reason: "Maono\u{00A0}AI Microphone -> Studio Display Speakers (blocked)"
            )]
        )
    }

    /// Blocked, and nowhere to go: the rule has no candidate, so the blocked device keeps the
    /// output rather than being switched to nothing.
    func testBlockedCurrentOutputWithNoListedDevicePresentDoesNothing() {
        var config = self.config
        config.blockedOutput = ["Maono AI Microphone"]
        let snapshot = snapshot([maonoSpeakers], current: maonoSpeakers)
        XCTAssertEqual(OutputPinRule.reconcile(snapshot, config), [])
    }

    /// Off the list and off the blocked list is a hand-picked output, blocked list or not.
    func testUnlistedUnblockedCurrentOutputIsStillLeftAlone() {
        var config = self.config
        config.blockedOutput = ["Maono AI Microphone"]
        let snapshot = snapshot([Fixture.airPods, Fixture.displaySpeakers], current: Fixture.airPods)
        XCTAssertEqual(OutputPinRule.reconcile(snapshot, config), [])
    }

    func testNoDefaultOutputDoesNothing() {
        let snapshot = snapshot([Fixture.displaySpeakers, Fixture.macSpeakers], current: nil)
        XCTAssertEqual(OutputPinRule.reconcile(snapshot, config), [])
    }
}
