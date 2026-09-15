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
        let wanted = config.launchAtLogin
        guard wanted != (service.status == .enabled) else { return }

        do {
            if wanted { try service.register() } else { try service.unregister() }
            note("launchAtLogin: launchd agent \(wanted ? "registered" : "unregistered")")
        } catch {
            note("launchAtLogin: \(wanted ? "register" : "unregister") failed (\(error.localizedDescription))")
        }
    }

    /// Versions up to 0.3.2 registered the app itself as a login item. An upgrade inherits that
    /// registration, and leaving it in place would start a second copy at login, so it goes first -
    /// whether or not the agent is wanted, since the answer to "do not start at login" has to
    /// cover the registration the previous version made. Once it is unregistered its status is no
    /// longer `.enabled`, so this runs once and then costs a status read per config reload.
    /// A daemon that LaunchServices started - Finder, `open`, the cask's postflight - steps aside
    /// for the one launchd supervises rather than running beside it. launchd spawns its copy
    /// directly, so LaunchServices' "this app is already running" never sees it, and two daemons
    /// would publish the same status file and hand CoreAudio the same writes twice.
    ///
    /// Two signals that fail independently have to agree before this process exits, because
    /// exiting the only daemon on the machine is the expensive mistake here: launchd's copy is
    /// named after the label (`LaunchAgent.wasStartedByLaunchd`), and the pid launchd reports for
    /// the job is not this process. A job that is loaded but not running is started rather than
    /// waited for, so the handover never leaves nothing behind.
    func handOverToLaunchAgentIfNeeded() {
        guard !LaunchAgent.wasStartedByLaunchd else { return }
        let job = LaunchAgent.loadedJob()
        guard job.isLoaded, job.pid != getpid() else { return }

        note("handing over to the launchd agent")
        LaunchAgent.launchctl(["kickstart", LaunchAgent.domainTarget])
        Darwin.exit(0)
    }

    private func retireLoginItem() {
        let loginItem = SMAppService.mainApp
        guard loginItem.status == .enabled else { return }
        do {
            try loginItem.unregister()
            note("launchAtLogin: old login item unregistered, the launchd agent replaces it")
        } catch {
            note("launchAtLogin: unregistering the old login item failed (\(error.localizedDescription))")
        }
    }
}
