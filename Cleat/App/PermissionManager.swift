import AVFoundation

/// Microphone TCC, the one permission Cleat needs and only for silence detection.
///
/// The dialog is the reason Cleat ships as an .app rather than a bare CLI on a LaunchAgent: a
/// command-line binary launched by launchd often never gets asked, and the request is denied
/// silently instead.
enum PermissionManager {

    static var microphoneStatus: AVAuthorizationStatus {
        AVCaptureDevice.authorizationStatus(for: .audio)
    }

    static func requestMicrophone() async -> Bool {
        switch microphoneStatus {
        case .authorized:
            return true
        case .notDetermined:
            return await AVCaptureDevice.requestAccess(for: .audio)
        default:
            return false
        }
    }

    static func describe(_ status: AVAuthorizationStatus) -> String {
        switch status {
        case .authorized: return "authorized"
        case .denied: return "denied"
        case .restricted: return "restricted"
        case .notDetermined: return "not determined"
        @unknown default: return "unknown"
        }
    }
}
