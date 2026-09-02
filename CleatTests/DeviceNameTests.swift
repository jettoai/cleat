import XCTest
@testable import Cleat

final class DeviceNameTests: XCTestCase {

    func testNoBreakSpaceMatchesPlainSpace() {
        // The reason this file exists: Maono's device name carries U+00A0, and a hand-typed
        // config entry never will.
        XCTAssertTrue(DeviceName.matches(
            entry: "Maono AI Microphone",
            name: "Maono\u{00A0}AI Microphone",
            uid: "AppleUSBAudioEngine:Maono:1"
        ))
    }

    func testNarrowNoBreakSpaceMatchesPlainSpace() {
        XCTAssertTrue(DeviceName.matches(
            entry: "Maono AI Microphone",
            name: "Maono\u{202F}AI Microphone",
            uid: "uid"
        ))
    }

    func testDifferentCaseDoesNotMatch() {
        XCTAssertFalse(DeviceName.matches(entry: "brio 100", name: "Brio 100", uid: "uid"))
    }

    func testExactUIDMatches() {
        let uid = "AppleUSBAudioEngine:Shenzhen Hollyland Technology Co.,Ltd:Wireless microphone:952X2D2Q952:3"
        XCTAssertTrue(DeviceName.matches(entry: uid, name: "Wireless microphone", uid: uid))
    }

    func testUIDIsComparedLiterallyNotNormalized() {
        // A UID is exact by construction, so normalising it would let two different UIDs collide.
        XCTAssertFalse(DeviceName.matches(entry: "uid:a", name: "Some Device", uid: "uid:b"))
    }

    func testSurroundingWhitespaceIsTrimmed() {
        XCTAssertTrue(DeviceName.matches(entry: "  Brio 100  ", name: "Brio 100", uid: "uid"))
    }

    func testRepeatedSpacesCollapse() {
        XCTAssertTrue(DeviceName.matches(entry: "Brio    100", name: "Brio 100", uid: "uid"))
    }

    func testEmptyEntryDoesNotMatchEmptyUID() {
        // An absent UID must not turn an empty config entry into a wildcard.
        XCTAssertFalse(DeviceName.matches(entry: "", name: "Brio 100", uid: ""))
    }

    func testNormalizeIsIdempotent() {
        let once = DeviceName.normalize("Maono\u{00A0}AI  Microphone ")
        XCTAssertEqual(once, "Maono AI Microphone")
        XCTAssertEqual(DeviceName.normalize(once), once)
    }

    func testUnrelatedNameDoesNotMatch() {
        XCTAssertFalse(DeviceName.matches(entry: "Wireless microphone", name: "Brio 100", uid: "uid"))
    }
}
