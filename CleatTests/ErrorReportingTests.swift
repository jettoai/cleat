import Sentry
import XCTest
@testable import Cleat

final class ErrorReportingTests: XCTestCase {

    func testScrubFoldsTheHomeDirectoryAndDropsTheUser() {
        let home = "/Users/someone"
        let event = Event(level: .error)
        event.user = User(userId: "installation")
        event.message = SentryMessage(formatted: "failed to read \(home)/.config/cleat/config.json")

        let frame = Frame()
        frame.package = "\(home)/Applications/Cleat.app/Contents/MacOS/Cleat"
        let exception = Exception(value: "crash in \(home)/Applications/Cleat.app", type: "EXC_BAD_ACCESS")
        exception.stacktrace = SentryStacktrace(frames: [frame], registers: [:])
        event.exceptions = [exception]

        let image = DebugMeta()
        image.codeFile = "\(home)/Applications/Cleat.app/Contents/MacOS/Cleat"
        event.debugMeta = [image]

        let scrubbed = ErrorReporting.scrub(event, home: home)

        XCTAssertNil(scrubbed.user)
        XCTAssertEqual(scrubbed.message?.formatted, "failed to read ~/.config/cleat/config.json")
        XCTAssertEqual(scrubbed.exceptions?.first?.value, "crash in ~/Applications/Cleat.app")
        XCTAssertEqual(
            scrubbed.exceptions?.first?.stacktrace?.frames.first?.package,
            "~/Applications/Cleat.app/Contents/MacOS/Cleat"
        )
        XCTAssertEqual(scrubbed.debugMeta?.first?.codeFile, "~/Applications/Cleat.app/Contents/MacOS/Cleat")
    }

    /// A status file written by a daemon that predates `errorReports` must still read as a daemon,
    /// not as no daemon at all.
    func testStatusFromAnOlderDaemonStillReads() throws {
        let url = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("cleat-old-status-\(UUID().uuidString).json")
        defer { try? FileManager.default.removeItem(at: url) }
        try Data("""
        {
          "pid": 123,
          "updatedAt": "2026-10-01T00:00:00Z",
          "configState": "ok",
          "microphone": "authorized",
          "rules": {},
          "liveness": {},
          "recentEvents": []
        }
        """.utf8).write(to: url)

        let status = try XCTUnwrap(StatusStore.read(from: url))
        XCTAssertNil(status.errorReports)
    }
}
