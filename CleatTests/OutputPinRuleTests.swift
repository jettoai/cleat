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

    // MARK: - Headphone takeover changes whose business a Bluetooth output is

    /// With takeover on, the list neither moves the output off a connected headset nor reaches
    /// for one. With takeover off, nothing about this rule changes.
    func testTakeoverOnLeavesABluetoothOutputAlone() {
        var config = self.config
        config.output = ["Studio Display Speakers", "AirPods Max"]
        let snapshot = snapshot([Fixture.airPods, Fixture.displaySpeakers], current: Fixture.airPods)

        config.headphonesTakeOver = true
        XCTAssertEqual(OutputPinRule.reconcile(snapshot, config), [])

        config.headphonesTakeOver = false
        XCTAssertEqual(
            OutputPinRule.reconcile(snapshot, config),
            [.setDefaultOutput(
                Fixture.displaySpeakers.id,
                reason: "AirPods Max -> Studio Display Speakers (higher priority present)"
            )]
        )
    }

    /// A listed headset is not a target either, or picking the speakers by hand would be undone.
    func testTakeoverOnDoesNotSwitchToAListedBluetoothDevice() {
        var config = self.config
        config.output = ["AirPods Max", "Studio Display Speakers", "MacBook Pro Speakers"]
        config.headphonesTakeOver = true
        // The AirPods top the list and are right here, so without the filter they would be the
        // target and the hand-picked speakers would be taken away.
        let snapshot = snapshot(
            [Fixture.airPods, Fixture.displaySpeakers, Fixture.macSpeakers], current: Fixture.macSpeakers
        )
        XCTAssertEqual(
            OutputPinRule.reconcile(snapshot, config),
            [.setDefaultOutput(
                Fixture.displaySpeakers.id,
                reason: "MacBook Pro Speakers -> Studio Display Speakers (higher priority present)"
            )]
        )
    }

    /// Blocked outranks "it is a headset": a Bluetooth device on the blocked list is still moved
    /// off the output.
    func testBlockedBluetoothDeviceIsStillReplacedWithTakeoverOn() {
        var config = self.config
        config.headphonesTakeOver = true
        config.blockedOutput = ["AirPods Max"]
        let snapshot = snapshot([Fixture.airPods, Fixture.displaySpeakers], current: Fixture.airPods)
        XCTAssertEqual(
            OutputPinRule.reconcile(snapshot, config),
            [.setDefaultOutput(
                Fixture.displaySpeakers.id,
                reason: "AirPods Max -> Studio Display Speakers (blocked)"
            )]
        )
    }

    /// Every listed output is a headset the rule may not touch, so the list has no candidate - but
    /// the current output is blocked, and that list is not suspended just because the priority
    /// list ran out. The sound moves to the first output present that is neither blocked nor a
    /// headset.
    func testBlockedCurrentOutputIsEvictedWhenEveryListedOutputIsAHeadset() {
        var config = Config(output: ["AirPods Max"])
        config.headphonesTakeOver = true
        config.blockedOutput = ["Maono AI Microphone"]
        let snapshot = snapshot(
            [maonoSpeakers, Fixture.airPods, Fixture.displaySpeakers], current: maonoSpeakers
        )
        XCTAssertEqual(
            OutputPinRule.reconcile(snapshot, config),
            [.setDefaultOutput(
                Fixture.displaySpeakers.id,
                reason: "Maono\u{00A0}AI Microphone -> Studio Display Speakers (blocked)"
            )]
        )
    }

    /// Nowhere to go: the only other output present is the headset itself, which this rule may not
    /// switch to. The blocked device keeps the output rather than the sound going nowhere.
    func testBlockedCurrentOutputStaysWhenTheOnlyEscapeIsAHeadset() {
        var config = Config(output: ["AirPods Max"])
        config.headphonesTakeOver = true
        config.blockedOutput = ["Maono AI Microphone"]
        let snapshot = snapshot([maonoSpeakers, Fixture.airPods], current: maonoSpeakers)
        XCTAssertEqual(OutputPinRule.reconcile(snapshot, config), [])
    }

    /// The escape is only for blocked outputs. A hand-picked one that is merely unlisted stays,
    /// exactly as it does when the list does have a candidate.
    func testUnlistedCurrentOutputIsLeftAloneWhenEveryListedOutputIsAHeadset() {
        var config = Config(output: ["AirPods Max"])
        config.headphonesTakeOver = true
        config.blockedOutput = ["Maono AI Microphone"]
        let snapshot = snapshot(
            [Fixture.macSpeakers, Fixture.airPods, Fixture.displaySpeakers], current: Fixture.macSpeakers
        )
        XCTAssertEqual(OutputPinRule.reconcile(snapshot, config), [])
    }

    func testNoDefaultOutputDoesNothing() {
        let snapshot = snapshot([Fixture.displaySpeakers, Fixture.macSpeakers], current: nil)
        XCTAssertEqual(OutputPinRule.reconcile(snapshot, config), [])
    }
}
