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
        Task {
            // Permission first: the rules that do not need a microphone start either way, and the
            // silence detector only runs if this comes back granted.
            let granted = await PermissionManager.requestMicrophone()
            engine.start(
                microphoneGranted: granted,
                microphoneState: PermissionManager.describe(PermissionManager.microphoneStatus)
            )
        }
    }
}
