import Darwin
import Foundation

/// `cleat status`, `cleat log`, `cleat version`.
///
/// The CLI never talks to the daemon; it reads the two files the daemon writes. That is the whole
/// design: no IPC to keep alive, and `cleat status` works the same whether the daemon is running,
/// wedged, or gone - it says which.
enum CLI {

    private static let defaultLogLines = 50

    static func run(_ arguments: [String]) -> Int32 {
        guard let command = arguments.first else { return usage() }

        switch command {
        case "status":
            return status()
        case "log":
            return log(Array(arguments.dropFirst()))
        case "version", "--version":
            print(version)
            return 0
        case "help", "--help", "-h":
            printUsage(to: FileHandle.standardOutput)
            return 0
        default:
            FileHandle.standardError.write(Data("cleat: unknown command '\(command)'\n".utf8))
            return usage()
        }
    }

    // MARK: - status

    private static func status() -> Int32 {
        guard let status = StatusStore.read() else {
            print("daemon:     not running (no status file at \(tildePath(Paths.statusURL)))")
            return 1
        }

        let running = isAlive(status.pid)
        print("daemon:     " + (running
            ? "running (pid \(status.pid))"
            : "not running (last seen as pid \(status.pid))"))
        print("updated:    \(display(status.updatedAt))")
        print("config:     \(status.configState) (\(tildePath(Paths.configURL)))")
        print("microphone: \(status.microphone)")
        print("input:      \(status.defaultInput ?? "-")")
        print("output:     \(status.defaultOutput ?? "-")")

        if !status.rules.isEmpty {
            print("rules:")
            for (name, verdict) in status.rules.sorted(by: { $0.key < $1.key }) {
                print("  \(pad(name)) \(verdict)")
            }
        }
        if !status.liveness.isEmpty {
            print("liveness:")
            for (device, state) in status.liveness.sorted(by: { $0.key < $1.key }) {
                print("  \(pad(device)) \(state)")
            }
        }
        if !status.recentEvents.isEmpty {
            print("recent:")
            for event in status.recentEvents.suffix(5) {
                print("  \(event)")
            }
        }

        return running ? 0 : 1
    }

    // MARK: - log

    private static func log(_ arguments: [String]) -> Int32 {
        var count = defaultLogLines
        var index = 0
        while index < arguments.count {
            let argument = arguments[index]
            if argument == "-n" || argument == "--lines" {
                guard index + 1 < arguments.count, let value = Int(arguments[index + 1]), value > 0 else {
                    FileHandle.standardError.write(Data("cleat log: -n needs a positive number\n".utf8))
                    return 2
                }
                count = value
                index += 2
            } else {
                FileHandle.standardError.write(Data("cleat log: unexpected argument '\(argument)'\n".utf8))
                return 2
            }
        }

        let lines = EventLog.tail(count)
        guard !lines.isEmpty else {
            print("no events yet (\(tildePath(Paths.logURL)))")
            return 0
        }
        lines.forEach { print($0) }
        return 0
    }

    // MARK: - Helpers

    private static var version: String {
        Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "unknown"
    }

    /// `kill(pid, 0)` asks whether the process exists without touching it. EPERM means it exists
    /// and belongs to somebody else, which still counts as running.
    private static func isAlive(_ pid: Int32) -> Bool {
        guard pid > 0 else { return false }
        if kill(pid, 0) == 0 { return true }
        return errno == EPERM
    }

    private static func pad(_ value: String) -> String {
        value.padding(toLength: max(12, value.count + 1), withPad: " ", startingAt: 0)
    }

    private static func tildePath(_ url: URL) -> String {
        let home = Paths.home.path
        return url.path.hasPrefix(home) ? "~" + url.path.dropFirst(home.count) : url.path
    }

    private static func display(_ date: Date) -> String {
        let formatter = DateFormatter()
        formatter.dateFormat = "yyyy-MM-dd HH:mm:ss"
        formatter.locale = Locale(identifier: "en_US_POSIX")
        return formatter.string(from: date)
    }

    @discardableResult
    private static func usage() -> Int32 {
        printUsage(to: FileHandle.standardError)
        return 2
    }

    private static func printUsage(to handle: FileHandle) {
        handle.write(Data("""
        cleat - audio device keeper

        usage:
          cleat status        what the daemon is holding right now
          cleat log [-n 50]   recent events
          cleat version

        Running Cleat.app with no arguments starts the daemon.

        """.utf8))
    }
}
