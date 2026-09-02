import CoreAudio
import Foundation

/// Rule 5: hold each listed microphone's input gain, against conferencing apps that "automatically
/// adjust microphone volume".
///
/// Deliberately not limited to the default input: Zoom lowers the gain of the device it is using,
/// which may not be the one macOS calls default, and the drift is only noticed the next time that
/// mic is picked.
enum InputVolumeRule {

    /// Percent. CoreAudio scalars round-trip imprecisely, so a hair of slack keeps a matched
    /// volume from being rewritten on every event.
    static let tolerance = 0.5

    static func reconcile(_ snapshot: DeviceSnapshot, _ config: Config) -> [Action] {
        // Sorted so the action list is deterministic - a dictionary's order is not.
        config.inputVolume.sorted { $0.key < $1.key }.compactMap { entry, wantedPercent in
            guard let device = snapshot.device(matching: entry, input: true) else { return nil }
            guard let current = snapshot.inputVolumes[device.id] else { return nil }

            let currentPercent = Double(current) * 100
            guard abs(currentPercent - wantedPercent) > tolerance else { return nil }

            let reason = String(
                format: "%@ %.0f%% -> %.0f%%", device.name, currentPercent, wantedPercent
            )
            return .setInputVolume(device.id, Float(wantedPercent / 100), reason: reason)
        }
    }
}
