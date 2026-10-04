import AppKit
import Foundation

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    static let shared = AppDelegate()

    // Error reporting starts and stops on the main thread (ErrorReporting); the engine calls this
    // on its own queue, and main.async keeps an on followed by an off in that order.
    private let engine = Engine(errorReportsChanged: { on in
        DispatchQueue.main.async { MainActor.assumeIsolated { ErrorReporting.apply(on) } }
    })

    func applicationDidFinishLaunching(_ notification: Notification) {
        // Unit tests are hosted in this app. Starting the engine there would register CoreAudio
        // listeners and pop the microphone dialog in the middle of a test run.
        guard ProcessInfo.processInfo.environment["XCTestConfigurationFilePath"] == nil else { return }

        let engine = self.engine
        // Engine first, dialog second. Four of the five rules need no microphone, and the moment
        // the TCC dialog is on screen is exactly when a user is plugging things in; waiting for an
        // answer that may never come would leave the balance and the volume locks off meanwhile.
        engine.start(microphone: PermissionManager.current)

        Task {
            // Only silence detection depends on the answer, and the engine attaches its detectors
            // when it arrives. An answer that changes nothing is not logged.
            engine.updateMicrophone(await PermissionManager.request())
        }
    }
}
