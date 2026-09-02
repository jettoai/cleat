import CoreAudio
import XCTest
@testable import Cleat

final class InputVolumeRuleTests: XCTestCase {

    private let config = Config(
        input: ["Wireless microphone", "Brio 100"],
        inputVolume: ["Wireless microphone": 88, "Brio 100": 75]
    )

    private func snapshot(_ volumes: [AudioDeviceID: Float], default defaultInput: AudioDeviceID? = Fixture.wireless.id) -> DeviceSnapshot {
        DeviceSnapshot(
            devices: [Fixture.wireless, Fixture.brio, Fixture.maono],
            defaultInput: defaultInput,
            inputVolumes: volumes
        )
    }

    func testMatchingVolumeDoesNothing() {
        let snapshot = snapshot([Fixture.wireless.id: 0.88, Fixture.brio.id: 0.75])
        XCTAssertEqual(InputVolumeRule.reconcile(snapshot, config), [])
    }

    func testDriftedVolumeIsPulledBack() {
        let snapshot = snapshot([Fixture.wireless.id: 0.60, Fixture.brio.id: 0.75])
        XCTAssertEqual(
            InputVolumeRule.reconcile(snapshot, config),
            [.setInputVolume(Fixture.wireless.id, 0.88, reason: "Wireless microphone 60% -> 88%")]
        )
    }

    func testNonDefaultDeviceIsAlsoHeld() {
        // Zoom lowers the gain of the device IT uses, which need not be the system default.
        let snapshot = snapshot([Fixture.wireless.id: 0.88, Fixture.brio.id: 0.40], default: Fixture.wireless.id)
        XCTAssertEqual(
            InputVolumeRule.reconcile(snapshot, config),
            [.setInputVolume(Fixture.brio.id, 0.75, reason: "Brio 100 40% -> 75%")]
        )
    }

    func testUnlistedDeviceIsLeftAlone() {
        let snapshot = snapshot([
            Fixture.wireless.id: 0.88, Fixture.brio.id: 0.75, Fixture.maono.id: 0.10
        ])
        XCTAssertEqual(InputVolumeRule.reconcile(snapshot, config), [])
    }

    func testDeviceWithNoReadingIsSkipped() {
        // The volume could not be read this pass; guessing would write a value nobody measured.
        let snapshot = snapshot([Fixture.brio.id: 0.75])
        XCTAssertEqual(InputVolumeRule.reconcile(snapshot, config), [])
    }

    func testAbsentDeviceIsSkipped() {
        let snapshot = DeviceSnapshot(
            devices: [Fixture.brio],
            defaultInput: Fixture.brio.id,
            inputVolumes: [Fixture.brio.id: 0.40]
        )
        XCTAssertEqual(
            InputVolumeRule.reconcile(snapshot, config),
            [.setInputVolume(Fixture.brio.id, 0.75, reason: "Brio 100 40% -> 75%")]
        )
    }

    func testMultipleDriftsAreOrderedDeterministically() {
        let snapshot = snapshot([Fixture.wireless.id: 0.10, Fixture.brio.id: 0.10])
        XCTAssertEqual(
            InputVolumeRule.reconcile(snapshot, config),
            [
                .setInputVolume(Fixture.brio.id, 0.75, reason: "Brio 100 10% -> 75%"),
                .setInputVolume(Fixture.wireless.id, 0.88, reason: "Wireless microphone 10% -> 88%")
            ]
        )
    }

    func testEmptyConfigDoesNothing() {
        let snapshot = snapshot([Fixture.wireless.id: 0.10])
        XCTAssertEqual(InputVolumeRule.reconcile(snapshot, Config()), [])
    }
}
