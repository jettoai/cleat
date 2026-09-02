import AppKit
import Foundation

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    static let shared = AppDelegate()

    private let engine = Engine()

    func applicationDidFinishLaunching(_ notification: Notification) {
        // Unit tests are hosted in this app. Starting the engine there would register CoreAudio
        // listeners and pop the microphone dialog in the middle of a test run.
        guard ProcessInfo.processInfo.environment["XCTestConfigurationFilePath"] == nil else { return }

        let engine = self.engine
        // Engine first, dialog second. Four of the five rules need no microphone, and the moment
        // the TCC dialog is on screen is exactly when a user is plugging things in; waiting for an
        // answer that may never come would leave the balance and the volume locks off meanwhile.
        let status = PermissionManager.microphoneStatus
        engine.start(
            microphoneGranted: status == .authorized,
            microphoneState: PermissionManager.describe(status)
        )

        Task {
            // Only silence detection depends on the answer, and the engine attaches its detectors
            // when it arrives. Already authorized means this changes nothing and is not logged.
            let granted = await PermissionManager.requestMicrophone()
            engine.updateMicrophone(
                granted: granted,
                state: PermissionManager.describe(PermissionManager.microphoneStatus)
            )
        }
    }
}
