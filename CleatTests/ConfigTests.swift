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
          "balance": 0.5,
          "inputVolume": { "Wireless microphone": 88, "Brio 100": 75 },
          "liveness": { "Wireless microphone": { "zeroSeconds": 3 } },
          "launchAtLogin": false
        }
        """)

        XCTAssertEqual(config.input, ["Wireless microphone", "Brio 100"])
        XCTAssertEqual(config.blockedInput, ["AirPods Max"])
        XCTAssertEqual(config.output, ["Studio Display Speakers"])
        XCTAssertEqual(config.balance, 0.5)
        XCTAssertEqual(config.inputVolume, ["Wireless microphone": 88, "Brio 100": 75])
        XCTAssertEqual(config.liveness, ["Wireless microphone": LivenessConfig(zeroSeconds: 3)])
        XCTAssertFalse(config.launchAtLogin)
        XCTAssertNoThrow(try config.validate())
    }

    func testMissingFieldsFallBackToDefaults() throws {
        let config = try decode("{}")

        XCTAssertEqual(config, Config.disabled)
        XCTAssertEqual(config.input, [])
        XCTAssertEqual(config.blockedInput, [])
        XCTAssertEqual(config.output, [])
        XCTAssertNil(config.balance)
        XCTAssertEqual(config.inputVolume, [:])
        XCTAssertEqual(config.liveness, [:])
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
