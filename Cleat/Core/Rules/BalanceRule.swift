import CoreAudio
import Foundation

/// Rule 3: hold the default output's left/right balance where the config says.
///
/// macOS drifts it off centre after some Bluetooth reconnects. A reading of `nil` means the device
/// is not ready yet rather than centred, so this rule does nothing and lets the engine's retry
/// beats come back to it.
enum BalanceRule {

    /// Anything closer than this is the same setting expressed in floating point.
    static let tolerance = 0.01

    static func reconcile(_ snapshot: DeviceSnapshot, _ config: Config) -> [Action] {
        guard let wanted = config.balance,
              let deviceID = snapshot.defaultOutput,
              let current = snapshot.outputBalance else { return [] }

        guard abs(Double(current) - wanted) > tolerance else { return [] }

        let name = snapshot.device(id: deviceID)?.name ?? "output"
        let reason = String(format: "%@ %.2f -> %.2f", name, Double(current), wanted)
        return [.setBalance(deviceID, Float(wanted), reason: reason)]
    }
}
