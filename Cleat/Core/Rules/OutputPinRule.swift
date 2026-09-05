import CoreAudio

/// Rule 4: the same priority list for outputs, without the liveness gate - an output sends no
/// signal to measure. A default output that is neither on the priority list nor on the blocked
/// list was chosen by hand, so it stays.
///
/// The blocked list exists because a USB microphone can carry a speaker end that macOS is happy to
/// fall back to when the current output leaves: the headphones go, and the sound comes out of the
/// microphone. Such a device is not on the priority list, so without the blocked list it would
/// read as a hand-picked output and be left there.
///
/// Which output is wanted while headphones are connected is not this rule's business; that is
/// `HeadphonesTakeoverRule`. With `headphonesTakeOver` on, a Bluetooth output is outside this
/// rule's jurisdiction in both directions: it is never the device this list switches to, and it is
/// never the device this list switches away from. Without that, listing a headset in `output` as
/// well would produce two audible bounces - the takeover on the settle beat undone by the retry
/// beat, and a hand-picked speaker taken back the moment the list ran again. The list means "what
/// plays when no headphones hold the output", and this is what makes that sentence true.
///
/// A blocked device is not headphones for this purpose. `blockedOutput` is the user saying "never
/// this one", which outranks "this one is a headset" - both when a headset asks to be the output
/// and when the list runs out of anywhere to send the sound instead.
enum OutputPinRule {

    static func reconcile(_ snapshot: DeviceSnapshot, _ config: Config) -> [Action] {
        guard !config.output.isEmpty else { return [] }

        let listed = config.output.compactMap { snapshot.device(matching: $0, input: false) }
        let candidates = listed.filter { !HeadphonesTakeoverRule.owns($0, config) }

        guard let currentID = snapshot.defaultOutput,
              let current = snapshot.device(id: currentID) else { return [] }

        guard let target = candidates.first else {
            return evictBlocked(current, snapshot, config, anyListedPresent: !listed.isEmpty)
        }

        guard currentID != target.id else { return [] }
        guard !HeadphonesTakeoverRule.owns(current, config) else { return [] }

        let isPinned = current.isListed(in: config.output)
        let isBlocked = current.isListed(in: config.blockedOutput)
        guard isPinned || isBlocked else { return [] }

        let cause = isBlocked ? "blocked" : "higher priority present"
        return [.setDefaultOutput(
            target.id, reason: "\(current.name) -> \(target.name) (\(cause))"
        )]
    }

    /// Every listed output that is here is a headset this rule may not switch to, so the priority
    /// list has nothing to offer. That is not a reason to leave the sound on a device the user
    /// blocked: `blockedOutput` is "never this one", and it outranks the list running out of
    /// candidates. The way out is the first output present that is neither blocked nor a headset,
    /// in name order, so the choice does not depend on the order the HAL handed the devices back.
    ///
    /// It is deliberately narrow. With no listed device here at all the rule stays silent as
    /// before: the list is what says where sound belongs, and a list that names nothing present
    /// has not said anything. And a current output that is merely unlisted was picked by hand, so
    /// only a blocked one is moved.
    private static func evictBlocked(
        _ current: AudioDevice,
        _ snapshot: DeviceSnapshot,
        _ config: Config,
        anyListedPresent: Bool
    ) -> [Action] {
        guard anyListedPresent, current.isListed(in: config.blockedOutput) else { return [] }

        let escapes = snapshot.devices.filter {
            $0.hasOutput
                && !$0.isListed(in: config.blockedOutput)
                && !HeadphonesTakeoverRule.owns($0, config)
        }
        guard let escape = escapes.min(by: AudioDevice.byName) else { return [] }

        return [.setDefaultOutput(
            escape.id, reason: "\(current.name) -> \(escape.name) (blocked)"
        )]
    }
}
