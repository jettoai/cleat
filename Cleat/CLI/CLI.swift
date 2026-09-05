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
        case "reclaim":
            return reclaim(Array(arguments.dropFirst()))
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

    // MARK: - reclaim

    /// Sends one routing request by hand and prints what came back. The daemon does this on its
    /// own when the Mac is playing; this is the button for the times it did not, and the way to
    /// find out what the arbitration is answering without reading the system log.
    ///
    /// A device may be named on the command line, which is how this is exercised without editing
    /// the config file. With no argument it asks for whatever `reclaim` lists.
    private static func reclaim(_ arguments: [String]) -> Int32 {
        let client = SmartRoutingClient()
        guard client.isAvailable else {
            return fail("reclaim: this macOS has no BTAudioRoutingRequest, the rule is off here")
        }

        let wanted: [String]
        if arguments.isEmpty {
            guard let config = try? Config.load(from: Paths.configURL) else {
                return fail("reclaim: no usable config at \(tildePath(Paths.configURL))")
            }
            guard !config.reclaim.isEmpty else {
                return fail("reclaim: nothing listed under \"reclaim\" in \(tildePath(Paths.configURL))")
            }
            wanted = config.reclaim
        } else {
            wanted = arguments
        }

        // The pairing list arrives in no particular order and a name can match more than one
        // headset, so the connected ones are what is picked from - in the rule's own order, so
        // this button and the daemon ask for the same headset - and the disconnected ones are
        // only what the message names when there is nothing to ask.
        let listed = ReclaimRule.inRuleOrder(
            IOBluetoothPairings().pairedHeadsets().filter { $0.isListed(in: wanted) }
        )
        guard !listed.isEmpty else {
            return fail("reclaim: none of \(wanted.joined(separator: ", ")) is paired with this Mac")
        }
        guard let target = listed.first(where: { $0.isConnected }) else {
            let names = listed.map(\.name).joined(separator: ", ")
            return fail(
                "reclaim: \(names) \(listed.count == 1 ? "is" : "are") paired but not connected"
            )
        }

        print("asking for \(target.name) (\(target.address)) with score \(Engine.reclaimScore)")

        let answered = DispatchSemaphore(value: 0)
        // The CLI is one short-lived process, so there is nothing to serialise against: the reply
        // lands on a queue of its own while this thread waits for it.
        let queue = DispatchQueue(label: "ai.jetto.cleat.cli.reclaim")
        let box = Answer()
        client.request(
            address: target.address,
            score: Engine.reclaimScore,
            reason: ReclaimRule.requestReason(for: target),
            queue: queue
        ) { response in
            box.value = response
            answered.signal()
        }

        guard answered.wait(timeout: .now() + responseTimeout) == .success, let answer = box.value else {
            return fail("reclaim: no answer in \(Int(responseTimeout))s")
        }

        let outcome = answer.outcome
        print("action:  \(answer.action.map(String.init) ?? "-")")
        print("reason:  \(answer.reason ?? "-")")
        if let error = answer.error { print("error:   \(error)") }
        print("verdict: \(verdict(outcome))")
        return outcome == .routed || outcome == .alreadyRouted ? 0 : 1
    }

    /// Observed in about ten milliseconds every time; the wait is this long only so a daemon that
    /// is wedged ends the command rather than the command ending the day.
    private static let responseTimeout: TimeInterval = 8

    /// The reply arrives on another queue, and the semaphore is what publishes it: a reference the
    /// waiting thread reads after the wait, rather than a captured variable written across queues.
    private final class Answer: @unchecked Sendable {
        var value: RouteResponse?
    }

    private static func verdict(_ outcome: RouteResponse.Outcome) -> String {
        switch outcome {
        case .routed: return "routed here"
        case .alreadyRouted: return "already here"
        case .heldByRemote(let detail): return "held by the remote device (\(detail))"
        case .busy: return "a previous request is still running"
        case .refused(let detail): return "refused (\(detail))"
        }
    }

    // MARK: - Helpers

    private static func fail(_ message: String) -> Int32 {
        FileHandle.standardError.write(Data("cleat \(message)\n".utf8))
        return 1
    }

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
          cleat reclaim [device]
                              ask a Bluetooth headset back from whatever took it, once,
                              and print the answer. Defaults to the config's "reclaim" list
          cleat version

        Running Cleat.app with no arguments starts the daemon.

        """.utf8))
    }
}
