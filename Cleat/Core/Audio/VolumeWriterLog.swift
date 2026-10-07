import Darwin
import Foundation

/// One "Volume update from PID" line from coreaudiod's Bluetooth audio driver
/// (`[com.apple.bluetooth:BTAudio] Volume update from PID = 8484 Control ID = 262 Scalar volume
/// 0.488189 -> 0.427210 Element = 0`). It is driver debug text, not an API: when it stops matching,
/// the hold must stop undoing anything rather than undo everything.
struct VolumeWrite: Equatable, Sendable {
    /// The log's own timestamp, so a line that arrives late is still placed where it happened.
    let at: Date
    let pid: Int32
    /// The writer's executable path as the reader resolved it ("pid N" when it had already
    /// exited). The engine replaces it with the config entry it matched, or its last component.
    var writer: String
    /// The driver's control id, logged only. 262/263 were the two output channels of AirPods Max,
    /// 261 the microphone; nothing here assumes which is which.
    let control: Int
    let from: Float
    let to: Float

    var changesValue: Bool { abs(from - to) > OutputVolumeHoldRule.tolerance }
}

/// What one stdout line of `log stream --style ndjson` turned out to be.
enum VolumeWriterEvent: Equatable, Sendable {
    case write(VolumeWrite)
    /// Passed the predicate but did not parse: the driver's wording changed.
    case unrecognised(String)
}

enum VolumeWriteParser {
    private static let pattern = try! NSRegularExpression(
        pattern: #"PID = (\d+) Control ID = (\d+) Scalar volume ([0-9.]+) -> ([0-9.]+)"#
    )
    private static let timestamp: DateFormatter = {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.dateFormat = "yyyy-MM-dd HH:mm:ss.SSSSSSZ"
        return formatter
    }()

    private struct Line: Decodable { let timestamp: String?; let eventMessage: String? }

    /// nil for lines that are not log events at all - `log stream` opens with a plain-text
    /// "Filtering the log data using ..." line, which is not a sign of anything.
    static func parse(ndjson line: String, writer: (Int32) -> String) -> VolumeWriterEvent? {
        guard let data = line.data(using: .utf8),
              let decoded = try? JSONDecoder().decode(Line.self, from: data),
              let message = decoded.eventMessage else { return nil }
        guard let stamp = decoded.timestamp.flatMap(timestamp.date(from:)),
              let parsed = parse(message: message) else { return .unrecognised(message) }
        return .write(VolumeWrite(at: stamp, pid: parsed.pid, writer: writer(parsed.pid),
                                  control: parsed.control, from: parsed.from, to: parsed.to))
    }

    static func parse(message: String) -> (pid: Int32, control: Int, from: Float, to: Float)? {
        let range = NSRange(message.startIndex..., in: message)
        guard let match = pattern.firstMatch(in: message, range: range) else { return nil }
        func group(_ i: Int) -> String? {
            Range(match.range(at: i), in: message).map { String(message[$0]) }
        }
        guard let pid = group(1).flatMap(Int32.init), let control = group(2).flatMap(Int.init),
              let from = group(3).flatMap(Float.init), let to = group(4).flatMap(Float.init)
        else { return nil }
        return (pid, control, from, to)
    }

    /// `proc_pidpath` of a process. Writers run as the same user (Parallels' prl_vm_app does), so
    /// no privilege is needed; a writer that has already exited is "pid N".
    static func executablePath(_ pid: Int32) -> String {
        var buffer = [CChar](repeating: 0, count: 4 * Int(MAXPATHLEN))
        guard proc_pidpath(pid, &buffer, UInt32(buffer.count)) > 0 else { return "pid \(pid)" }
        return String(cString: buffer)
    }
}

/// Where writer lines come from. A protocol so the engine can be driven by a script in tests.
protocol VolumeWriterSource: AnyObject {
    /// Starts delivering events on `queue`. False when the stream could not be started at all.
    func start(queue: DispatchQueue,
               onEvent: @escaping @Sendable (VolumeWriterEvent) -> Void,
               onExit: @escaping @Sendable (Int32) -> Void) -> Bool
    func stop()
}

/// `/usr/bin/log stream` as a child process. Measured 2026-10-07: runs as an ordinary user,
/// 1ms median / 17ms worst from the driver writing the line to it arriving here, 0.15% CPU.
final class VolumeWriterLog: VolumeWriterSource, @unchecked Sendable {

    static let predicate = #"process == "coreaudiod" AND eventMessage CONTAINS "Volume update from PID""#

    private var process: Process?
    private var buffer = Data()   // touched only on the pipe's reader thread

    func start(queue: DispatchQueue,
               onEvent: @escaping @Sendable (VolumeWriterEvent) -> Void,
               onExit: @escaping @Sendable (Int32) -> Void) -> Bool {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/log")
        process.arguments = ["stream", "--style", "ndjson", "--predicate", Self.predicate]
        let pipe = Pipe()
        process.standardOutput = pipe
        process.standardError = FileHandle.nullDevice
        process.standardInput = FileHandle.nullDevice

        // One reading method only (readabilityHandler), never mixed with a blocking read: mixing
        // the two on one fd is how a reader ends up waiting on data it already pulled.
        pipe.fileHandleForReading.readabilityHandler = { [weak self] handle in
            let chunk = handle.availableData
            guard let self, !chunk.isEmpty else { return }
            self.buffer.append(chunk)
            while let newline = self.buffer.firstIndex(of: 0x0A) {
                let lineData = self.buffer[self.buffer.startIndex..<newline]
                self.buffer.removeSubrange(self.buffer.startIndex...newline)
                guard let line = String(data: lineData, encoding: .utf8),
                      // The writer is named here, on arrival: a short-lived writer may be gone by
                      // the time the engine queue gets to it.
                      let event = VolumeWriteParser.parse(ndjson: line, writer: VolumeWriteParser.executablePath)
                else { continue }
                queue.async { onEvent(event) }
            }
        }
        process.terminationHandler = { finished in
            pipe.fileHandleForReading.readabilityHandler = nil
            let status = finished.terminationStatus
            queue.async { onExit(status) }
        }
        Self.reapOrphans()
        do { try process.run() } catch {
            pipe.fileHandleForReading.readabilityHandler = nil
            return false
        }
        self.process = process
        return true
    }

    /// A Cleat that died without running `stop()` (SIGKILL, crash, `cleat restart`) leaves its
    /// `log stream` reparented to launchd. Only those are killed: a reader whose Cleat is still
    /// alive (Dev and release side by side) has a real parent, not pid 1.
    private static func reapOrphans() {
        let pkill = Process()
        pkill.executableURL = URL(fileURLWithPath: "/usr/bin/pkill")
        pkill.arguments = ["-P", "1", "-U", String(getuid()), "-f", "^/usr/bin/log stream .*Volume update from PID"]
        pkill.standardOutput = FileHandle.nullDevice
        pkill.standardError = FileHandle.nullDevice
        guard (try? pkill.run()) != nil else { return }
        pkill.waitUntilExit()
    }

    func stop() {
        guard let process else { return }
        self.process = nil
        process.terminationHandler = nil
        if process.isRunning { process.terminate() }
    }
}
