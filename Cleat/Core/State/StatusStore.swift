import Foundation

/// What the daemon publishes after every reconcile, and the only thing `cleat status` reads.
///
/// A status file with no live process behind it is indistinguishable from a stale one, so the pid
/// travels with it and the CLI checks that the process is still there.
struct Status: Codable, Equatable {
    var pid: Int32
    var updatedAt: Date
    /// `ok`, `missing`, or `invalid (<reason>)`.
    var configState: String
    /// `authorized`, `denied`, `restricted`, or `not determined`.
    var microphone: String
    var defaultInput: String?
    var defaultOutput: String?
    /// Rule name to a one-line verdict, e.g. `inputPin` -> `on, holding Wireless microphone`.
    var rules: [String: String]
    /// Device name to `live` / `silent` / `measuring`.
    var liveness: [String: String]
    var recentEvents: [String]

    init(
        pid: Int32 = ProcessInfo.processInfo.processIdentifier,
        updatedAt: Date = Date(),
        configState: String = "missing",
        microphone: String = "not determined",
        defaultInput: String? = nil,
        defaultOutput: String? = nil,
        rules: [String: String] = [:],
        liveness: [String: String] = [:],
        recentEvents: [String] = []
    ) {
        self.pid = pid
        self.updatedAt = updatedAt
        self.configState = configState
        self.microphone = microphone
        self.defaultInput = defaultInput
        self.defaultOutput = defaultOutput
        self.rules = rules
        self.liveness = liveness
        self.recentEvents = recentEvents
    }
}

/// Atomic writes only: `cleat status` may read this file at any instant, and a half-written JSON
/// document would read as a crashed daemon.
enum StatusStore {

    private static let encoder: JSONEncoder = {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        encoder.dateEncodingStrategy = .iso8601
        return encoder
    }()

    private static let decoder: JSONDecoder = {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        return decoder
    }()

    static func write(_ status: Status, to url: URL = Paths.statusURL) {
        let manager = FileManager.default
        let directory = url.deletingLastPathComponent()
        try? manager.createDirectory(at: directory, withIntermediateDirectories: true)

        guard let data = try? encoder.encode(status) else { return }
        let temporary = directory.appendingPathComponent(".status-\(UUID().uuidString).json")
        guard (try? data.write(to: temporary)) != nil else { return }

        if manager.fileExists(atPath: url.path) {
            _ = try? manager.replaceItemAt(url, withItemAt: temporary)
        } else if (try? manager.moveItem(at: temporary, to: url)) == nil {
            try? manager.removeItem(at: temporary)
        }
    }

    static func read(from url: URL = Paths.statusURL) -> Status? {
        guard let data = try? Data(contentsOf: url) else { return nil }
        return try? decoder.decode(Status.self, from: data)
    }
}
