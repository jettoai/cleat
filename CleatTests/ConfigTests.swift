import XCTest
@testable import Cleat

final class ConfigTests: XCTestCase {

    private func decode(_ json: String) throws -> Config {
        try JSONDecoder().decode(Config.self, from: Data(json.utf8))
    }

    func testFullConfigDecodes() throws {
        let config = try decode("""
        {
          "input": ["Wireless microphone", "Brio 100"],
          "blockedInput": ["AirPods Max"],
          "output": ["Studio Display Speakers"],
          "blockedOutput": ["Maono AI Microphone"],
          "headphonesTakeOver": true,
          "balance": 0.5,
          "inputVolume": { "Wireless microphone": 88, "Brio 100": 75 },
          "liveness": { "Wireless microphone": { "zeroSeconds": 3 } },
          "reclaim": ["AirPods Max"],
          "launchAtLogin": false
        }
        """)

        XCTAssertEqual(config.input, ["Wireless microphone", "Brio 100"])
        XCTAssertEqual(config.blockedInput, ["AirPods Max"])
        XCTAssertEqual(config.output, ["Studio Display Speakers"])
        XCTAssertEqual(config.blockedOutput, ["Maono AI Microphone"])
        XCTAssertTrue(config.headphonesTakeOver)
        XCTAssertEqual(config.balance, 0.5)
        XCTAssertEqual(config.inputVolume, ["Wireless microphone": 88, "Brio 100": 75])
        XCTAssertEqual(config.liveness, ["Wireless microphone": LivenessConfig(zeroSeconds: 3)])
        XCTAssertEqual(config.reclaim, ["AirPods Max"])
        XCTAssertFalse(config.launchAtLogin)
        XCTAssertNoThrow(try config.validate())
    }

    func testMissingFieldsFallBackToDefaults() throws {
        let config = try decode("{}")

        XCTAssertEqual(config, Config.disabled)
        XCTAssertEqual(config.input, [])
        XCTAssertEqual(config.blockedInput, [])
        XCTAssertEqual(config.output, [])
        XCTAssertEqual(config.blockedOutput, [])
        // Off unless asked for: a config that says nothing must not start moving the output when
        // headphones connect.
        XCTAssertFalse(config.headphonesTakeOver)
        XCTAssertNil(config.balance)
        XCTAssertEqual(config.inputVolume, [:])
        XCTAssertEqual(config.liveness, [:])
        // Off unless asked for, same as headphone takeover: a config that says nothing must not
        // start asking a phone for a headset.
        XCTAssertEqual(config.reclaim, [])
        // launchAtLogin is the one default that is on: a config file that says nothing still
        // means "keep Cleat running".
        XCTAssertTrue(config.launchAtLogin)
    }

    func testPartialConfigKeepsOtherDefaults() throws {
        let config = try decode(#"{"input": ["Brio 100"]}"#)

        XCTAssertEqual(config.input, ["Brio 100"])
        XCTAssertNil(config.balance)
        XCTAssertTrue(config.launchAtLogin)
    }

    /// The key on its own, without the output list the other output rule needs: takeover does not
    /// read `output` at all, so this is a complete config for it.
    func testHeadphonesTakeOverDecodesOnItsOwn() throws {
        let config = try decode(#"{"headphonesTakeOver": true}"#)

        XCTAssertTrue(config.headphonesTakeOver)
        XCTAssertEqual(config.output, [])
        XCTAssertNoThrow(try config.validate())
    }

    func testMalformedJSONThrows() {
        XCTAssertThrowsError(try decode("{ this is not json "))
    }

    func testWrongTypeThrows() {
        XCTAssertThrowsError(try decode(#"{"input": "Brio 100"}"#))
    }

    func testBalanceOutOfRangeFailsValidation() throws {
        let config = try decode(#"{"balance": 1.5}"#)
        XCTAssertThrowsError(try config.validate()) { error in
            XCTAssertEqual(error as? ConfigError, .balanceOutOfRange(1.5))
        }
    }

    func testNegativeBalanceFailsValidation() throws {
        let config = try decode(#"{"balance": -0.2}"#)
        XCTAssertThrowsError(try config.validate())
    }

    func testVolumeOutOfRangeFailsValidation() throws {
        let config = try decode(#"{"inputVolume": {"Brio 100": 140}}"#)
        XCTAssertThrowsError(try config.validate()) { error in
            XCTAssertEqual(error as? ConfigError, .volumeOutOfRange(device: "Brio 100", value: 140))
        }
    }

    func testZeroSecondsBelowOneFailsValidation() throws {
        let config = try decode(#"{"liveness": {"Brio 100": {"zeroSeconds": 0.5}}}"#)
        XCTAssertThrowsError(try config.validate()) { error in
            XCTAssertEqual(error as? ConfigError, .zeroSecondsTooSmall(device: "Brio 100", value: 0.5))
        }
    }

    // MARK: - Input volume wildcard

    func testWildcardVolumeDecodes() throws {
        let config = try decode(#"{"inputVolume": {"*": 100, "Brio 100": 75}}"#)

        XCTAssertEqual(config.inputVolume, ["*": 100, "Brio 100": 75])
        XCTAssertTrue(config.inputVolumeHasWildcard)
        XCTAssertNoThrow(try config.validate())
    }

    func testWildcardOutOfRangeFailsValidation() throws {
        let config = try decode(#"{"inputVolume": {"*": 140}}"#)
        XCTAssertThrowsError(try config.validate()) { error in
            XCTAssertEqual(error as? ConfigError, .volumeOutOfRange(device: "*", value: 140))
        }
    }

    func testNamedTargetWinsOverWildcard() throws {
        let config = try decode(#"{"inputVolume": {"*": 100, "Brio 100": 75}}"#)
        XCTAssertEqual(config.inputVolumeTarget(for: Fixture.brio), 75)
    }

    func testUnnamedDeviceTakesTheWildcardTarget() throws {
        let config = try decode(#"{"inputVolume": {"*": 100, "Brio 100": 75}}"#)
        XCTAssertEqual(config.inputVolumeTarget(for: Fixture.airPods), 100)
    }

    /// A UID entry names the device just as a name does, and still beats the wildcard.
    func testUIDEntryWinsOverWildcard() throws {
        let config = Config(inputVolume: ["*": 100, Fixture.airPods.uid: 60])
        XCTAssertEqual(config.inputVolumeTarget(for: Fixture.airPods), 60)
    }

    func testWithoutWildcardAnUnnamedDeviceHasNoTarget() throws {
        let config = try decode(#"{"inputVolume": {"Brio 100": 75}}"#)
        XCTAssertFalse(config.inputVolumeHasWildcard)
        XCTAssertNil(config.inputVolumeTarget(for: Fixture.airPods))
        XCTAssertEqual(config.inputVolumeTarget(for: Fixture.brio), 75)
    }

    func testEmptyInputVolumeHasNoTargets() {
        XCTAssertFalse(Config.disabled.inputVolumeHasWildcard)
        XCTAssertNil(Config.disabled.inputVolumeTarget(for: Fixture.brio))
    }

    func testLoadFromFileRoundTrips() throws {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("cleat-config-\(UUID().uuidString).json")
        try Data(#"{"input": ["Brio 100"], "balance": 0.5}"#.utf8).write(to: url)
        defer { try? FileManager.default.removeItem(at: url) }

        let config = try Config.load(from: url)
        XCTAssertEqual(config.input, ["Brio 100"])
        XCTAssertEqual(config.balance, 0.5)
    }

    func testLoadMissingFileThrows() {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("cleat-absent-\(UUID().uuidString).json")
        XCTAssertThrowsError(try Config.load(from: url))
    }
}
