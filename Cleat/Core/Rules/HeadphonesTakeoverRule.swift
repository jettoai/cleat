import CoreAudio

/// Rule 6: a pair of Bluetooth headphones becomes the output the moment it connects.
///
/// This is the one rule that is driven by an event rather than by a state. macOS already does it
/// for wired headphones, and iOS does it for AirPods; over Bluetooth on a Mac nothing does, so
/// reconnecting a headset that was last used with a phone leaves the sound coming out of the
/// speakers. Connecting them is the request.
///
/// Being edge triggered is what keeps it from fighting the user. A state driven version - "while
/// headphones are here, they are the output" - would take the output back a second after the user
/// picked the speakers by hand with the headphones still on their head. Here the arrival is spent
/// on the pass that sees it: `snapshot.arrived` only carries devices that were absent on the
/// previous pass, so the retry beats that follow a device change do not fire this rule a second
/// time, and a hand-picked output afterwards stands until the headphones leave and come back.
///
/// It deliberately ignores `config.output`: the priority list decides what plays when no
/// headphones are around, and a headset does not have to be listed to be wanted.
enum HeadphonesTakeoverRule {

    static func reconcile(_ snapshot: DeviceSnapshot, _ config: Config) -> [Action] {
        guard config.headphonesTakeOver else { return [] }

        // Two headsets connecting on the same pass is a tie the name order breaks, the same way
        // every other list in Cleat is ordered, so the outcome does not depend on what order the
        // HAL happened to hand the devices back.
        let candidates = snapshot.devices
            .filter { snapshot.arrived.contains($0.uid) && $0.hasOutput && $0.isBluetooth }
            .sorted(by: AudioDevice.byName)

        guard let target = candidates.first else { return [] }
        guard snapshot.defaultOutput != target.id else { return [] }

        let current = snapshot.defaultOutput.flatMap { snapshot.device(id: $0)?.name } ?? "-"
        return [.setDefaultOutput(
            target.id, reason: "\(current) -> \(target.name) (headphones connected)"
        )]
    }
}
