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

    // MARK: - Wildcard

    /// `{"*": 100, "Brio 100": 75}`: the named entry wins for the device it names.
    func testNamedEntryOverridesTheWildcard() {
        let snapshot = DeviceSnapshot(
            devices: [Fixture.brio],
            defaultInput: Fixture.brio.id,
            inputVolumes: [Fixture.brio.id: 0.40]
        )
        XCTAssertEqual(
            InputVolumeRule.reconcile(snapshot, Self.wildcardWithOverride),
            [.setInputVolume(Fixture.brio.id, 0.75, reason: "Brio 100 40% -> 75%")]
        )
    }

    /// `{"*": 100, "Brio 100": 75}`: a device no entry names falls back to the wildcard - including
    /// a blocked one, which is a side effect of the wildcard rather than an oversight.
    func testUnnamedDeviceFallsBackToTheWildcard() {
        let snapshot = DeviceSnapshot(
            devices: [Fixture.airPods],
            defaultInput: Fixture.airPods.id,
            inputVolumes: [Fixture.airPods.id: 0.50]
        )
        XCTAssertEqual(
            InputVolumeRule.reconcile(snapshot, Self.wildcardWithOverride),
            [.setInputVolume(Fixture.airPods.id, 1.0, reason: "AirPods Max 50% -> 100%")]
        )
    }

    /// `{"Brio 100": 75}`: without a wildcard an unnamed device is left alone, exactly as before.
    func testWithoutWildcardAnUnnamedDeviceIsUntouched() {
        let config = Config(inputVolume: ["Brio 100": 75])
        let snapshot = DeviceSnapshot(
            devices: [Fixture.airPods],
            defaultInput: Fixture.airPods.id,
            inputVolumes: [Fixture.airPods.id: 0.50]
        )
        XCTAssertEqual(InputVolumeRule.reconcile(snapshot, config), [])
    }

    /// `{"*": 100}` and a device whose gain could not be read - a virtual device, say. Nothing is
    /// written and nothing is logged, because no action is produced at all.
    func testWildcardSkipsADeviceWithNoReading() {
        let config = Config(inputVolume: ["*": 100])
        let snapshot = DeviceSnapshot(
            devices: [Fixture.zoom, Fixture.brio],
            defaultInput: Fixture.brio.id,
            inputVolumes: [Fixture.brio.id: 1.0]
        )
        XCTAssertEqual(InputVolumeRule.reconcile(snapshot, config), [])
    }

    /// The wildcard walks devices rather than config keys, so its order is the device names'.
    func testWildcardActionsAreOrderedByDeviceName() {
        let config = Config(inputVolume: ["*": 100])
        let snapshot = DeviceSnapshot(
            devices: [Fixture.wireless, Fixture.brio, Fixture.airPods, Fixture.displaySpeakers],
            defaultInput: Fixture.brio.id,
            inputVolumes: [Fixture.wireless.id: 0.10, Fixture.brio.id: 0.20, Fixture.airPods.id: 0.30]
        )
        XCTAssertEqual(
            InputVolumeRule.reconcile(snapshot, config),
            [
                .setInputVolume(Fixture.airPods.id, 1.0, reason: "AirPods Max 30% -> 100%"),
                .setInputVolume(Fixture.brio.id, 1.0, reason: "Brio 100 20% -> 100%"),
                .setInputVolume(Fixture.wireless.id, 1.0, reason: "Wireless microphone 10% -> 100%")
            ]
        )
    }

    /// An output-only device has no input gain to hold, wildcard or not.
    func testWildcardIgnoresOutputOnlyDevices() {
        let config = Config(inputVolume: ["*": 100])
        let snapshot = DeviceSnapshot(
            devices: [Fixture.displaySpeakers],
            inputVolumes: [Fixture.displaySpeakers.id: 0.20]
        )
        XCTAssertEqual(InputVolumeRule.reconcile(snapshot, config), [])
    }

    /// A device already at the wildcard target is not rewritten every event.
    func testWildcardLeavesAMatchedDeviceAlone() {
        let config = Config(inputVolume: ["*": 100])
        let snapshot = DeviceSnapshot(
            devices: [Fixture.brio],
            defaultInput: Fixture.brio.id,
            inputVolumes: [Fixture.brio.id: 1.0]
        )
        XCTAssertEqual(InputVolumeRule.reconcile(snapshot, config), [])
    }

    private static let wildcardWithOverride = Config(inputVolume: ["*": 100, "Brio 100": 75])
}
