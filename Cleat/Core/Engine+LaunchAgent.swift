import Darwin
import Foundation
import ServiceManagement

/// Who is responsible for keeping the daemon alive. Split out of Engine.swift to keep both files
/// short; every function here runs on `Engine.queue`, same as the rest of the engine.
extension Engine {

    // MARK: - Launch agent

    /// What `launchAtLogin` registers is the launchd agent inside the bundle, not the app as a
    /// login item: launchd starts it at login *and* starts it again when it is killed or crashes,
    /// which a login item does not (`LaunchAgent`).
    ///
    /// The dev build never touches any of this: it shares no bundle id with the release build, so
    /// registering it would leave a second Cleat starting at login on the developer's machine.
    func syncLaunchAtLogin() {
        guard !LaunchAgent.isDevelopmentBuild else { return }
        guard configState == "ok" else { return }

        retireLoginItem()

        let service = LaunchAgent.service
        let status = service.status
        let wanted = config.launchAtLogin
        if wanted, LaunchAgent.needsReregistration(status: status, jobLoaded: LaunchAgent.loadedJob().isLoaded) {
            do {
                try service.unregister()
                try service.register()
                note("launchAtLogin: launchd agent re-registered (registered but not loaded)")
            } catch {
                note("launchAtLogin: re-register failed (\(error.localizedDescription))")
            }
            return
        }
        // Wanting it means wanting it running, which only `.enabled` is; not wanting it means
        // wanting no registration left behind, which `.requiresApproval` still is
        // (`LaunchAgent.isRegistered`).
        let needsChange = wanted ? status != .enabled : LaunchAgent.isRegistered(status)
        guard needsChange else { return }

        do {
            if wanted { try service.register() } else { try service.unregister() }
            note("launchAtLogin: launchd agent \(wanted ? "registered" : "unregistered")")
        } catch {
            note("launchAtLogin: \(wanted ? "register" : "unregister") failed (\(error.localizedDescription))")
        }
    }

    /// A daemon that LaunchServices started - Finder, `open`, the cask's postflight - steps aside
    /// for the one launchd supervises rather than running beside it. launchd spawns its copy
    /// directly, so LaunchServices' "this app is already running" never sees it, and two daemons
    /// would publish the same status file and hand CoreAudio the same writes twice.
    ///
    /// Two signals that fail independently have to agree before this process exits, because
    /// exiting the only daemon on the machine is the expensive mistake here: launchd's copy is
    /// named after the label (`LaunchAgent.wasStartedByLaunchd`), and the pid launchd reports for
    /// the job is not this process.
    ///
    /// What this process exits in favour of has to exist first. A job launchd reports a pid for is
    /// already up, so this one just steps aside. A job that is loaded but not running is kickstarted
    /// instead, and only a kickstart launchctl reports as successful is worth exiting for - when it
    /// fails, this daemon stays up rather than trading itself for nothing.
    func handOverToLaunchAgentIfNeeded() {
        guard !LaunchAgent.wasStartedByLaunchd else { return }
        let job = LaunchAgent.loadedJob()
        guard job.isLoaded, job.pid != getpid() else { return }

        if let pid = job.pid {
            note("handing over to the launchd agent (pid \(pid))")
            Darwin.exit(0)
        }

        let result = LaunchAgent.launchctl(["kickstart", LaunchAgent.domainTarget])
        guard result.status == 0 else {
            note("handing over to the launchd agent failed, staying up (\(LaunchAgent.failureDetail(result)))")
            return
        }

        note("handed over to the launchd agent")
        Darwin.exit(0)
    }

    /// Versions up to 0.3.2 registered the app itself as a login item. An upgrade inherits that
    /// registration, and leaving it in place would start a second copy at login, so it goes first -
    /// whether or not the agent is wanted, since the answer to "do not start at login" has to
    /// cover the registration the previous version made. An approval the person never cleared is
    /// one of those registrations (`LaunchAgent.isRegistered`), and it would start that second copy
    /// the moment they did. Once it is unregistered its status is neither `.enabled` nor
    /// `.requiresApproval`, so this runs once and then costs a status read per config reload.
    private func retireLoginItem() {
        let loginItem = SMAppService.mainApp
        guard LaunchAgent.isRegistered(loginItem.status) else { return }
        do {
            try loginItem.unregister()
            note("launchAtLogin: old login item unregistered, the launchd agent replaces it")
        } catch {
            note("launchAtLogin: unregistering the old login item failed (\(error.localizedDescription))")
        }
    }
}
