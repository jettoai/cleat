import CoreAudio
import Foundation

/// Rule 5: hold each configured microphone's input gain, against conferencing apps that
/// "automatically adjust microphone volume".
///
/// Deliberately not limited to the default input: Zoom lowers the gain of the device it is using,
/// which may not be the one macOS calls default, and the drift is only noticed the next time that
/// mic is picked.
///
/// The walk is over the devices rather than over the config entries, because `"*"` gives a target
/// to devices no entry names. A device whose gain could not be read is skipped: guessing would
/// write a value nobody measured.
enum InputVolumeRule {

    /// Percent. CoreAudio scalars round-trip imprecisely, so a hair of slack keeps a matched
    /// volume from being rewritten on every event.
    static let tolerance = 0.5

    static func reconcile(_ snapshot: DeviceSnapshot, _ config: Config) -> [Action] {
        guard !config.inputVolume.isEmpty else { return [] }

        // Sorted so the action list is deterministic - the device list's order is not.
        let devices = snapshot.devices.filter(\.hasInput).sorted(by: AudioDevice.byName)

        return devices.compactMap { device in
            guard let wantedPercent = config.inputVolumeTarget(for: device) else { return nil }
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
