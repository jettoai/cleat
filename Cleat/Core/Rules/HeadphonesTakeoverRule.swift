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

    /// Whether this rule owns the device: a Bluetooth output while takeover is on. `OutputPinRule`
    /// asks the same question before deciding whether the priority list may move the output, so
    /// the two rules cannot drift into disagreeing about which devices are headphones.
    ///
    /// `blockedOutput` wins over "it is a headset": that list is the user saying "never this one",
    /// without qualification.
    static func owns(_ device: AudioDevice, _ config: Config) -> Bool {
        config.headphonesTakeOver
            && device.isBluetooth
            && !device.isListed(in: config.blockedOutput)
    }

    /// The headsets this rule would act on, most preferred first. Two connecting on the same pass
    /// is a tie the name order breaks, the same way every other list in Cleat is ordered, so the
    /// outcome does not depend on what order the HAL happened to hand the devices back.
    ///
    /// Honouring `blockedOutput` here rather than leaving it to the next beat is what keeps that
    /// list's promise real: grabbing the device and letting another rule drop it half a second
    /// later would keep it only on paper.
    static func eligibleArrivals(_ snapshot: DeviceSnapshot, _ config: Config) -> [AudioDevice] {
        guard config.headphonesTakeOver else { return [] }
        return snapshot.devices
            .filter { snapshot.arrived.contains($0.uid) && $0.hasOutput && owns($0, config) }
            .sorted(by: AudioDevice.byName)
    }

    /// Whether this pass belongs to a headset that just arrived. The engine asks before running
    /// `OutputPinRule`, because "the rule wrote nothing" and "there was nothing to take over" are
    /// different things: when macOS has already moved the output to the arriving headset, this
    /// rule is quiet, and the priority list must not read that silence as permission to move the
    /// output somewhere else.
    static func hasEligibleArrival(_ snapshot: DeviceSnapshot, _ config: Config) -> Bool {
        !eligibleArrivals(snapshot, config).isEmpty
    }

    static func reconcile(_ snapshot: DeviceSnapshot, _ config: Config) -> [Action] {
        guard let target = eligibleArrivals(snapshot, config).first else { return [] }
        guard snapshot.defaultOutput != target.id else { return [] }

        let current = snapshot.defaultOutput.flatMap { snapshot.device(id: $0)?.name } ?? "-"
        return [.setDefaultOutput(
            target.id, reason: "\(current) -> \(target.name) (headphones connected)"
        )]
    }
}
