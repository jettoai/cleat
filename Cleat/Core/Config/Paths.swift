import Foundation

/// Every file Cleat reads or writes. The daemon and the CLI are the same binary, so both sides
/// resolve these from one place - a second spelling of any of them is a CLI that reports on a
/// status file nobody writes.
enum Paths {
    static var home: URL { FileManager.default.homeDirectoryForCurrentUser }

    /// The environment variable that moves the config file somewhere else.
    static let configOverrideVariable = "CLEAT_CONFIG"

    /// `~/.config/cleat/` - the user-owned side, edited by hand.
    static var configDirectory: URL { home.appendingPathComponent(".config/cleat", isDirectory: true) }

    /// `~/.config/cleat/config.json`, unless `CLEAT_CONFIG` names another file.
    ///
    /// The override is there so a dev build can be pointed at a throwaway config and enforce
    /// nothing on the machine it is being tested on. It moves this path only: status and log stay
    /// where they are, so `cleat status` reports on the same files whichever config was loaded,
    /// and a stray variable can never hide the real daemon's state behind a second copy.
    static var configURL: URL {
        guard let override = ProcessInfo.processInfo.environment[configOverrideVariable],
              !override.isEmpty
        else { return configDirectory.appendingPathComponent("config.json") }
        return URL(fileURLWithPath: (override as NSString).expandingTildeInPath)
    }

    /// `~/Library/Application Support/Cleat/` - what the daemon publishes for `cleat status`.
    static var supportDirectory: URL {
        home.appendingPathComponent("Library/Application Support/Cleat", isDirectory: true)
    }
    static var statusURL: URL { supportDirectory.appendingPathComponent("status.json") }

    /// `~/Library/Logs/Cleat/` - the append-only event log behind `cleat log`.
    static var logDirectory: URL { home.appendingPathComponent("Library/Logs/Cleat", isDirectory: true) }
    static var logURL: URL { logDirectory.appendingPathComponent("cleat.log") }
    static var rotatedLogURL: URL { logDirectory.appendingPathComponent("cleat.log.1") }
}
