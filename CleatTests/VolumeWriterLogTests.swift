import XCTest
@testable import Cleat

/// The writer line parser, fed the text coreaudiod actually logged on 2026-10-07.
final class VolumeWriterLogTests: XCTestCase {

    /// Parallels' first write of the day, as `log show --style ndjson` printed it.
    private let parallelsLine = #"{"timezoneName":"","messageType":"Default","eventType":"logEvent","source":null,"formatString":"Volume update from PID = %d Control ID = %d Scalar volume %f -> %f Element = %d","subsystem":"com.apple.bluetooth","category":"BTAudio","processImagePath":"\/usr\/sbin\/coreaudiod","senderImagePath":"\/System\/Library\/Audio\/Plug-Ins\/HAL\/BTAudioHALPlugin.driver\/Contents\/MacOS\/BTAudioHALPlugin","timestamp":"2026-10-07 10:19:55.713745+0800","eventMessage":"Volume update from PID = 8484 Control ID = 262 Scalar volume 0.488189 -> 0.427210 Element = 0","processID":342}"#

    private func ndjson(_ message: String) -> String {
        #"{"timestamp":"2026-10-07 10:19:55.713745+0800","eventMessage":"\#(message)"}"#
    }

    func testParsesTheRealParallelsLine() throws {
        let event = VolumeWriteParser.parse(ndjson: parallelsLine) { "name of \($0)" }
        guard case .write(let write) = event else { return XCTFail("not a write: \(String(describing: event))") }
        XCTAssertEqual(write.pid, 8484)
        XCTAssertEqual(write.writer, "name of 8484")
        XCTAssertEqual(write.control, 262)
        XCTAssertEqual(write.from, 0.488189, accuracy: 1e-6)
        XCTAssertEqual(write.to, 0.427210, accuracy: 1e-6)
        // 02:19:55.713 UTC, milliseconds kept.
        XCTAssertEqual(write.at.timeIntervalSince1970, 1_791_339_595.713, accuracy: 0.001)
        XCTAssertTrue(write.changesValue)
    }

    func testHeaderAndBlankLinesAreNotEventsButForeignWordingIsUnrecognised() {
        XCTAssertNil(VolumeWriteParser.parse(ndjson: #"Filtering the log data using "process == \"coreaudiod\"""#) { _ in "" })
        XCTAssertNil(VolumeWriteParser.parse(ndjson: "") { _ in "" })
        let crown = ndjson("A2DP : Volume received from bluetoothd: volume 0.488189")
        XCTAssertEqual(VolumeWriteParser.parse(ndjson: crown) { _ in "" },
                       .unrecognised("A2DP : Volume received from bluetoothd: volume 0.488189"))
    }

    func testOneChangedWordMakesTheLineUnrecognised() {
        let message = "Volume update from PID = 8484 Control ID = 262 Sclar volume 0.488189 -> 0.427210 Element = 0"
        XCTAssertEqual(VolumeWriteParser.parse(ndjson: ndjson(message)) { _ in "" }, .unrecognised(message))
    }

    func testExecutablePaths() {
        let host = Bundle.main.executableURL?.resolvingSymlinksInPath().path
        XCTAssertNotNil(host)
        XCTAssertEqual(VolumeWriteParser.executablePath(getpid()), host)
        XCTAssertEqual(VolumeWriteParser.executablePath(99_999_999), "pid 99999999")
    }

    /// An entry names the executable or an app it lives in; nothing else matches.
    func testListedEntryMatchesExecutableOrApp() {
        let parallels = "/Applications/Parallels Desktop.app/Contents/MacOS/Parallels VM.app/Contents/MacOS/prl_vm_app"
        let chrome = "/Applications/Google Chrome.app/Contents/Frameworks/Google Chrome Framework.framework/"
            + "Versions/141.0/Helpers/Google Chrome Helper (Renderer).app/Contents/MacOS/Google Chrome Helper (Renderer)"
        let match = OutputVolumeHoldRule.listedEntry(path:in:)

        XCTAssertEqual(match(parallels, ["prl_vm_app"]), "prl_vm_app")
        XCTAssertEqual(match(parallels, ["Parallels Desktop"]), "Parallels Desktop")
        XCTAssertEqual(match(parallels, ["Parallels VM"]), "Parallels VM")
        XCTAssertEqual(match(chrome, ["Jetto", "Google Chrome"]), "Google Chrome")

        XCTAssertNil(match(parallels, ["Parallels"]))          // a prefix of the app name is not the app
        XCTAssertNil(match(parallels, ["MacOS"]))              // a folder is not an app
        XCTAssertNil(match(parallels, ["prl_vm"]))
        XCTAssertNil(match(parallels, [""]))
        XCTAssertNil(match("/System/Library/CoreServices/ControlCenter.app/Contents/MacOS/ControlCenter",
                           ["Parallels Desktop", "prl_vm_app"]))
        XCTAssertNil(match("pid 8484", ["Parallels Desktop"]))
    }
}
