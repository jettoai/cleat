import Darwin
import Foundation
import ServiceManagement

/// The launchd agent shipped inside the bundle, named and talked to from one place so the daemon
/// that registers it, the daemon that steps aside for it and the CLI that kickstarts it can never
/// disagree about what it is called.
///
/// A login item (`SMAppService.mainApp`) is only started at login, so anything that killed the
/// daemon during the day - a crash, another process sweeping up its children - left it dead until
/// the next login with nothing to say so. The agent's `KeepAlive` is what brings it back.
enum LaunchAgent {

    /// The launchd label, the plist's file name inside the bundle, and the bundle identifier are
    /// all this one string. The fallback is never reached in a built bundle; it is here so this is
    /// not an optional every caller has to unwrap.
    static let label = Bundle.main.bundleIdentifier ?? "ai.jetto.cleat"

    static let plistName = label + ".plist"

    static var service: SMAppService { SMAppService.agent(plistName: plistName) }

    /// What "registered" means for anything registered through SMAppService - this bundle's agent,
    /// and the login item versions up to 0.3.2 left behind. `.requiresApproval` is registered: the
    /// registration exists and only a person clearing it in Login Items turns it on. Reading that
    /// state as "not registered" would leave a `launchAtLogin` switched back off unable to take the
    /// registration away, would leave the old login item in place, and would have `cleat restart`
    /// register what is registered already - so every caller asks this one question instead of
    /// comparing against `.enabled` itself.
    static func isRegistered(_ status: SMAppService.Status) -> Bool {
        switch status {
        case .enabled, .requiresApproval: return true
        default: return false
        }
    }

    /// Whether a registration has to be made again because launchd lost the job behind it.
    ///
    /// Seen on a Homebrew upgrade (0.3.5 to 0.3.6): the old cask's `uninstall launchctl` removed
    /// the job, but the background-task registration survived, so `status` still said `.enabled`.
    /// Everything that trusted that - the daemon's sync, `cleat restart`, `cleat status` - then
    /// left a daemon running with nothing supervising it. Only unregistering and registering again
    /// brings the job back. `.requiresApproval` is not this case: what it needs is a person, and
    /// registering over it only throws.
    static func needsReregistration(status: SMAppService.Status, jobLoaded: Bool) -> Bool {
        status == .enabled && !jobLoaded
    }

    /// The launchd domain target for `launchctl`: the agent lives in the logged-in user's GUI
    /// session, which is the only place an app that may need to show a dialog belongs.
    static let domainTarget = "gui/\(getuid())/\(label)"

    /// The dev build shares no bundle identifier with the release build, so registering it would
    /// leave a second Cleat starting at login on the developer's machine. It is driven by hand
    /// (`launchctl bootstrap` with a plist of one's own) when the supervision itself is what is
    /// being tested.
    static let isDevelopmentBuild = label.hasSuffix(".dev")

    /// Whether launchd started this process rather than LaunchServices.
    ///
    /// Measured on the dev build rather than assumed: launchd names the job's service after the
    /// label (`XPC_SERVICE_NAME=ai.jetto.cleat.dev`), while LaunchServices makes a name up for
    /// every launch (`application.ai.jetto.cleat.dev.375547097.475943345`).
    static let wasStartedByLaunchd = ProcessInfo.processInfo.environment["XPC_SERVICE_NAME"] == label

    /// What launchd has for the agent right now: whether the job is loaded at all, and the pid it
    /// is running as if it is up.
    ///
    /// This asks launchd rather than `SMAppService.status` on purpose. It is the question "is
    /// something already supervising a Cleat", which a hand-bootstrapped job answers too, and the
    /// answer decides whether a second daemon exits - so it has to be about jobs that exist, not
    /// about registrations this bundle made.
    static func loadedJob() -> (isLoaded: Bool, pid: pid_t?) {
        let result = launchctl(["print", domainTarget])
        guard result.status == 0 else { return (false, nil) }
        return (true, firstPID(in: result.output))
    }

    /// `launchctl print` puts the job's own `pid = N` above the per-endpoint entries, so the first
    /// one is the job's. A job that is loaded but not running prints none at all.
    private static func firstPID(in output: String) -> pid_t? {
        let prefix = "pid = "
        for line in output.split(separator: "\n") {
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            guard trimmed.hasPrefix(prefix) else { continue }
            return pid_t(trimmed.dropFirst(prefix.count).trimmingCharacters(in: .whitespaces))
        }
        return nil
    }

    /// Runs `launchctl` and returns what it said. Output is read before the wait on purpose: a
    /// pipe filled by a process nobody is reading from is a process that never exits.
    @discardableResult
    static func launchctl(_ arguments: [String]) -> (status: Int32, output: String) {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/bin/launchctl")
        process.arguments = arguments
        let pipe = Pipe()
        process.standardOutput = pipe
        process.standardError = pipe

        do {
            try process.run()
        } catch {
            return (-1, error.localizedDescription)
        }
        let output = pipe.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        return (
            process.terminationStatus,
            String(decoding: output, as: UTF8.self).trimmingCharacters(in: .whitespacesAndNewlines)
        )
    }

    /// What to say about a `launchctl` call that did not work: whatever it printed, or its exit
    /// status when it printed nothing at all.
    static func failureDetail(_ result: (status: Int32, output: String)) -> String {
        result.output.isEmpty ? "launchctl exited \(result.status)" : result.output
    }
}
