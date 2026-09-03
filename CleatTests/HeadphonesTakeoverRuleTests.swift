import CoreAudio
import XCTest
@testable import Cleat

/// Rule 6, one test per row of the decision table in the spec (section 2.5).
///
/// The rule reads one thing the others do not: `arrived`, the set of devices that were not here on
/// the previous pass. Every test therefore has to say both what is present and what is new, and
/// the last two say what happens when those two disagree.
final class HeadphonesTakeoverRuleTests: XCTestCase {

    private let config = Config(
        output: ["Studio Display Speakers"], headphonesTakeOver: true
    )

    /// A second pair of Bluetooth headphones, sorting after the AirPods Max by name.
    private let airPodsPro = AudioDevice(
        id: 31, name: "AirPods Pro", uid: "AirPodsPro-UID",
        hasInput: true, hasOutput: true, transport: kAudioDeviceTransportTypeBluetooth
    )

    /// A Bluetooth device with an input side only - a headset in the middle of negotiating, or a
    /// microphone. Nothing to send audio to, so nothing to take over.
    private let bluetoothMic = AudioDevice(
        id: 32, name: "Bluetooth Mic", uid: "BluetoothMic-UID",
        hasInput: true, hasOutput: false, transport: kAudioDeviceTransportTypeBluetoothLE
    )

    private func snapshot(
        _ devices: [AudioDevice], current: AudioDevice?, arrived: [AudioDevice]
    ) -> DeviceSnapshot {
        DeviceSnapshot(
            devices: devices,
            defaultOutput: current?.id,
            arrived: Set(arrived.map(\.uid))
        )
    }

    func testArrivedBluetoothOutputTakesOver() {
        let snapshot = snapshot(
            [Fixture.macStudioSpeakers, Fixture.airPods],
            current: Fixture.macStudioSpeakers,
            arrived: [Fixture.airPods]
        )
        XCTAssertEqual(
            HeadphonesTakeoverRule.reconcile(snapshot, config),
            [.setDefaultOutput(
                Fixture.airPods.id,
                reason: "Mac Studio的揚聲器 -> AirPods Max (headphones connected)"
            )]
        )
    }

    /// Wired headphones and USB speakers arrive the same way; macOS already switches to the jack,
    /// and this rule is only about the gap Bluetooth leaves.
    func testArrivedWiredOutputIsNotTakenOver() {
        let snapshot = snapshot(
            [Fixture.macStudioSpeakers, Fixture.wiredHeadphones],
            current: Fixture.macStudioSpeakers,
            arrived: [Fixture.wiredHeadphones]
        )
        XCTAssertEqual(HeadphonesTakeoverRule.reconcile(snapshot, config), [])
    }

    func testArrivedBluetoothInputOnlyDeviceIsNotAnOutput() {
        let snapshot = snapshot(
            [Fixture.macStudioSpeakers, bluetoothMic],
            current: Fixture.macStudioSpeakers,
            arrived: [bluetoothMic]
        )
        XCTAssertEqual(HeadphonesTakeoverRule.reconcile(snapshot, config), [])
    }

    /// The headphones arrived and macOS put the output on them by itself. Nothing left to do, and
    /// nothing to log.
    func testArrivedHeadphonesAlreadyTheDefaultOutputDoNothing() {
        let snapshot = snapshot(
            [Fixture.macStudioSpeakers, Fixture.airPods],
            current: Fixture.airPods,
            arrived: [Fixture.airPods]
        )
        XCTAssertEqual(HeadphonesTakeoverRule.reconcile(snapshot, config), [])
    }

    func testRuleOffDoesNothing() {
        let snapshot = snapshot(
            [Fixture.macStudioSpeakers, Fixture.airPods],
            current: Fixture.macStudioSpeakers,
            arrived: [Fixture.airPods]
        )
        var config = self.config
        config.headphonesTakeOver = false
        XCTAssertEqual(HeadphonesTakeoverRule.reconcile(snapshot, config), [])
    }

    /// Two headsets on one pass is decided by name, not by the order the HAL listed them.
    func testTwoHeadphonesArrivingTogetherResolveByName() {
        let snapshot = snapshot(
            [Fixture.macStudioSpeakers, airPodsPro, Fixture.airPods],
            current: Fixture.macStudioSpeakers,
            arrived: [airPodsPro, Fixture.airPods]
        )
        XCTAssertEqual(
            HeadphonesTakeoverRule.reconcile(snapshot, config),
            [.setDefaultOutput(
                Fixture.airPods.id,
                reason: "Mac Studio的揚聲器 -> AirPods Max (headphones connected)"
            )]
        )
    }

    /// The row that proves the rule is edge triggered: the headphones are right here, and the user
    /// has since picked the speakers by hand. They arrived on some earlier pass, so this one leaves
    /// the choice alone.
    func testPresentButNotArrivedHeadphonesAreLeftAlone() {
        let snapshot = snapshot(
            [Fixture.macStudioSpeakers, Fixture.airPods],
            current: Fixture.macStudioSpeakers,
            arrived: []
        )
        XCTAssertEqual(HeadphonesTakeoverRule.reconcile(snapshot, config), [])
    }

    /// Nothing holds the default output yet, which is what a snapshot taken while the previous
    /// output is being unplugged looks like. There is no name to put on the left of the reason,
    /// and the switch happens anyway.
    func testNoDefaultOutputStillTakesOver() {
        let snapshot = snapshot(
            [Fixture.airPods], current: nil, arrived: [Fixture.airPods]
        )
        XCTAssertEqual(
            HeadphonesTakeoverRule.reconcile(snapshot, config),
            [.setDefaultOutput(Fixture.airPods.id, reason: "- -> AirPods Max (headphones connected)")]
        )
    }
}
