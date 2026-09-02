import Foundation

/// The append-only log behind `cleat log`.
///
/// Only actions that changed something are written. A reconcile that finds everything already in
/// place writes nothing, so the log is a list of what Cleat did rather than proof that it is
/// awake - which is what makes it readable a week later.
final class EventLog: @unchecked Sendable {

    /// Rotate at a megabyte, keeping exactly one older file. A device-flapping cable can write a
    /// lot of lines, and this log is a diagnostic, not an archive.
    private static let maxBytes = 1_048_576

    private let url: URL
    private let rotatedURL: URL
    private let lock = NSLock()

    private let formatter: DateFormatter = {
        let formatter = DateFormatter()
        formatter.dateFormat = "yyyy-MM-dd HH:mm:ss"
        formatter.locale = Locale(identifier: "en_US_POSIX")
        return formatter
    }()

    init(url: URL = Paths.logURL, rotatedURL: URL = Paths.rotatedLogURL) {
        self.url = url
        self.rotatedURL = rotatedURL
    }

    @discardableResult
    func append(_ message: String) -> String {
        let line = "\(formatter.string(from: Date())) \(message)"
        lock.lock()
        defer { lock.unlock() }

        let manager = FileManager.default
        try? manager.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        rotateIfNeeded(manager)

        guard let data = (line + "\n").data(using: .utf8) else { return line }
        if let handle = try? FileHandle(forWritingTo: url) {
            defer { try? handle.close() }
            _ = try? handle.seekToEnd()
            try? handle.write(contentsOf: data)
        } else {
            try? data.write(to: url)
        }
        return line
    }

    private func rotateIfNeeded(_ manager: FileManager) {
        guard let attributes = try? manager.attributesOfItem(atPath: url.path),
              let size = attributes[.size] as? Int, size > Self.maxBytes else { return }
        try? manager.removeItem(at: rotatedURL)
        try? manager.moveItem(at: url, to: rotatedURL)
    }

    /// Last `count` lines, for `cleat log -n`.
    static func tail(_ count: Int, url: URL = Paths.logURL) -> [String] {
        guard let text = try? String(contentsOf: url, encoding: .utf8) else { return [] }
        let lines = text.split(separator: "\n", omittingEmptySubsequences: false)
            .map(String.init)
            .filter { !$0.isEmpty }
        return Array(lines.suffix(count))
    }
}
