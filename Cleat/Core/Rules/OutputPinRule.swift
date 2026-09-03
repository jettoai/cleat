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
/// this one", which outranks "this one is a headset".
enum OutputPinRule {

    static func reconcile(_ snapshot: DeviceSnapshot, _ config: Config) -> [Action] {
        guard !config.output.isEmpty else { return [] }

        let candidates = config.output
            .compactMap { snapshot.device(matching: $0, input: false) }
            .filter { !HeadphonesTakeoverRule.owns($0, config) }
        guard let target = candidates.first else { return [] }

        guard let currentID = snapshot.defaultOutput, currentID != target.id else { return [] }
        guard let current = snapshot.device(id: currentID) else { return [] }
        guard !HeadphonesTakeoverRule.owns(current, config) else { return [] }

        let isPinned = current.isListed(in: config.output)
        let isBlocked = current.isListed(in: config.blockedOutput)
        guard isPinned || isBlocked else { return [] }

        let cause = isBlocked ? "blocked" : "higher priority present"
        return [.setDefaultOutput(
            target.id, reason: "\(current.name) -> \(target.name) (\(cause))"
        )]
    }
}
