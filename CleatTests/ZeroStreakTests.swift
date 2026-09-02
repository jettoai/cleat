import XCTest
@testable import Cleat

final class ZeroStreakTests: XCTestCase {

    func testNineZeroFramesBelowThresholdDoNotFlip() {
        var streak = ZeroStreak(thresholdFrames: 10)
        XCTAssertNil(streak.feed(frames: 9, allZero: true))
        XCTAssertFalse(streak.isSilent)
    }

    func testThresholdReachedFlipsToSilent() {
        var streak = ZeroStreak(thresholdFrames: 10)
        XCTAssertNil(streak.feed(frames: 9, allZero: true))
        XCTAssertEqual(streak.feed(frames: 1, allZero: true), .becameSilent)
        XCTAssertTrue(streak.isSilent)
    }

    func testExactlyThresholdInOneBufferFlips() {
        var streak = ZeroStreak(thresholdFrames: 10)
        XCTAssertEqual(streak.feed(frames: 10, allZero: true), .becameSilent)
    }

    func testStayingSilentDoesNotFlipAgain() {
        var streak = ZeroStreak(thresholdFrames: 10)
        XCTAssertEqual(streak.feed(frames: 10, allZero: true), .becameSilent)
        XCTAssertNil(streak.feed(frames: 10, allZero: true))
        XCTAssertNil(streak.feed(frames: 1000, allZero: true))
    }

    func testOneNonZeroBufferFlipsBackImmediately() {
        var streak = ZeroStreak(thresholdFrames: 10)
        XCTAssertEqual(streak.feed(frames: 10, allZero: true), .becameSilent)
        XCTAssertEqual(streak.feed(frames: 1, allZero: false), .becameLive)
        XCTAssertFalse(streak.isSilent)
    }

    func testFirstBufferReportsLive() {
        // Otherwise a device that is fine from the start reads as "measuring" forever.
        var streak = ZeroStreak(thresholdFrames: 10)
        XCTAssertEqual(streak.feed(frames: 512, allZero: false), .becameLive)
    }

    func testLiveStateDoesNotFlipOnMoreSignal() {
        var streak = ZeroStreak(thresholdFrames: 10)
        XCTAssertEqual(streak.feed(frames: 512, allZero: false), .becameLive)
        XCTAssertNil(streak.feed(frames: 512, allZero: false))
    }

    func testPartialSilenceResetsTheCount() {
        var streak = ZeroStreak(thresholdFrames: 10)
        XCTAssertNil(streak.feed(frames: 9, allZero: true))
        XCTAssertEqual(streak.feed(frames: 1, allZero: false), .becameLive)
        XCTAssertEqual(streak.zeroFrames, 0)
        // The count restarts, so nine more zero frames are still not enough.
        XCTAssertNil(streak.feed(frames: 9, allZero: true))
    }

    func testThresholdIsAtLeastOneFrame() {
        var streak = ZeroStreak(thresholdFrames: 0)
        XCTAssertEqual(streak.thresholdFrames, 1)
        XCTAssertEqual(streak.feed(frames: 1, allZero: true), .becameSilent)
    }
}
