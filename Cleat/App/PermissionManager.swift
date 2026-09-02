import AVFoundation

/// Microphone TCC, the one permission Cleat needs and only for silence detection.
///
/// The dialog is the reason Cleat ships as an .app rather than a bare CLI on a LaunchAgent: a
/// command-line binary launched by launchd often never gets asked, and the request is denied
/// silently instead.
enum PermissionManager {

    /// What TCC says right now, without asking anybody anything.
    static var current: MicrophonePermission {
        permission(for: AVCaptureDevice.authorizationStatus(for: .audio))
    }

    /// Shows the dialog if it has never been answered, then reports the state it left behind.
    /// Reading the status afterwards rather than trusting the returned Bool keeps one source for
    /// the answer, including the "restricted" case that is never asked at all.
    static func request() async -> MicrophonePermission {
        if current == .pending {
            _ = await AVCaptureDevice.requestAccess(for: .audio)
        }
        return current
    }

    static func permission(for status: AVAuthorizationStatus) -> MicrophonePermission {
        switch status {
        case .authorized: return .granted
        case .notDetermined: return .pending
        case .denied: return .denied("denied")
        case .restricted: return .denied("restricted")
        @unknown default: return .denied("unknown")
        }
    }
}
