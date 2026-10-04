import Foundation
import Sentry

/// Opt-in crash and error reporting through Sentry.
///
/// Off by default, and off means the SDK is never started: no SentrySDK call runs, so nothing is
/// sent and nothing is written. `"errorReports": true` in config.json is the only way on; the
/// engine passes every change of that setting here (`Engine.syncErrorReports`). When on, events
/// carry stack traces, the app version and the macOS version; no user, no breadcrumbs, and the
/// home directory path is folded to `~` before sending.
///
/// Daemon only: the CLI half of the binary exits in main.swift before anything here can run.
/// Main thread only: off the main thread SentrySDK.start installs its hub asynchronously, and
/// SentrySDK.close blocks on the main queue (sentry-cocoa 9.29.1, SentrySDKInternal.m), so the
/// type is main-actor isolated. That also keeps it out of the realtime IOProc, which is not.
@MainActor
enum ErrorReporting {
    /// Dev probe: `-CleatSentryTestEvent YES` sends one message once the SDK is running. With
    /// reporting off it sends nothing, which is the check that "off" means off.
    static let testEventFlag = "CleatSentryTestEvent"

    private static let dsn =
        "https://5f28269bfd8a449ab060365e4870d043@o4508371539263488.ingest.us.sentry.io/4512198218088448"

    private static var started = false
    private static var probeSent = false

    static func apply(_ on: Bool) {
        if on { start() } else { stop() }
    }

    private static func start() {
        guard !started else { return }
        started = true
        let info = Bundle.main.infoDictionary ?? [:]
        let bundleID = Bundle.main.bundleIdentifier ?? "ai.jetto.cleat"
        let version = info["CFBundleShortVersionString"] as? String ?? "0"
        let build = info["CFBundleVersion"] as? String ?? "0"
        SentrySDK.start { options in
            options.dsn = dsn
            options.sendDefaultPii = false
            options.releaseName = "\(bundleID)@\(version)+\(build)"
            #if DEBUG
            options.environment = "development"
            #else
            options.environment = "production"
            #endif
            options.tracesSampleRate = 0
            options.enableAutoSessionTracking = false
            // A daemon with no UI: nobody sees the main thread stall, and every work item runs on
            // Engine.queue rather than here. Hang events would only be noise.
            options.enableAppHangTracking = false
            options.enableCaptureFailedRequests = false
            options.maxBreadcrumbs = 0
            options.beforeSend = { event in scrub(event) }
        }
        sendProbeIfAsked()
    }

    private static func stop() {
        guard started else { return }
        started = false
        SentrySDK.close()
    }

    private static func sendProbeIfAsked() {
        guard !probeSent, UserDefaults.standard.bool(forKey: testEventFlag), SentrySDK.isEnabled
        else { return }
        probeSent = true
        SentrySDK.capture(message: "cleat sentry probe \(ISO8601DateFormatter().string(from: Date()))")
        DispatchQueue.global(qos: .utility).async { SentrySDK.flush(timeout: 5) }
    }

    /// Folds the home directory to `~` in every field a path lands in. The crash converter writes
    /// full binary image paths into `debugMeta[].codeFile` and each frame's `package`, so an app run
    /// from ~/Applications would otherwise carry the login name. Field names are the ones in
    /// sentry-cocoa 9.29.1's public headers (SentryDebugMeta.h, SentryFrame.h). The user is dropped
    /// because the SDK fills it with an installation id (SentryClient.m).
    nonisolated static func scrub(_ event: Event, home: String = NSHomeDirectory()) -> Event {
        event.user = nil
        func fold(_ text: String) -> String { text.replacingOccurrences(of: home, with: "~") }
        func foldFrames(_ trace: SentryStacktrace?) {
            trace?.frames.forEach { frame in
                frame.package = frame.package.map(fold)
                frame.fileName = frame.fileName.map(fold)
                frame.module = frame.module.map(fold)
            }
        }
        if let message = event.message {
            let folded = SentryMessage(formatted: fold(message.formatted))
            folded.message = message.message.map(fold)
            folded.params = message.params
            event.message = folded
        }
        event.exceptions?.forEach {
            $0.value = $0.value.map(fold)
            foldFrames($0.stacktrace)
        }
        event.threads?.forEach { foldFrames($0.stacktrace) }
        foldFrames(event.stacktrace)
        event.debugMeta?.forEach { $0.codeFile = $0.codeFile.map(fold) }
        return event
    }
}
