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
/// `HeadphonesTakeoverRule`, and the engine runs this one only when that one had nothing to say.
enum OutputPinRule {

    static func reconcile(_ snapshot: DeviceSnapshot, _ config: Config) -> [Action] {
        guard !config.output.isEmpty else { return [] }

        let candidates = config.output.compactMap { snapshot.device(matching: $0, input: false) }
        guard let target = candidates.first else { return [] }

        guard let currentID = snapshot.defaultOutput, currentID != target.id else { return [] }
        guard let current = snapshot.device(id: currentID) else { return [] }

        let isPinned = current.isListed(in: config.output)
        let isBlocked = current.isListed(in: config.blockedOutput)
        guard isPinned || isBlocked else { return [] }

        let cause = isBlocked ? "blocked" : "higher priority present"
        return [.setDefaultOutput(
            target.id, reason: "\(current.name) -> \(target.name) (\(cause))"
        )]
    }
}
