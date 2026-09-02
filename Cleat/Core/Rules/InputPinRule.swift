import CoreAudio

/// Rule 1 (with the rule 2 gate): keep the default input on the most preferred microphone that is
/// plugged in and actually sending signal.
///
/// The one thing this rule must not do is take over a device the user chose by hand. So it only
/// acts when the current default is itself on the priority list (a switch between our own devices)
/// or on the blocked list (macOS grabbed the input when AirPods connected). Anything else - Zoom's
/// virtual device, Teams', a mic picked in System Settings - is left alone.
enum InputPinRule {

    static func reconcile(_ snapshot: DeviceSnapshot, _ config: Config) -> [Action] {
        guard !config.input.isEmpty else { return [] }

        // Candidates in priority order, skipping anything unplugged.
        let candidates = config.input.compactMap { snapshot.device(matching: $0, input: true) }

        // The liveness gate. A receiver whose transmitter is off is still a CoreAudio device, so a
        // device measured as silent is skipped outright. A device still measuring is skipped only
        // as a *destination*: it may keep the default input it already holds, but nothing switches
        // to it before the verdict is in. Otherwise every restart with the transmitter off switches
        // to the receiver and back three seconds later, which is two device changes in the middle
        // of whatever the user was doing. The live verdict arrives on the first non-zero buffer,
        // about a tenth of a second, so waiting for it costs nothing.
        guard let target = candidates.first(where: { candidate in
            switch snapshot.liveness[candidate.uid] {
            case .silent: return false
            case .measuring: return candidate.id == snapshot.defaultInput
            case .live, nil: return true
            }
        }) else {
            return []
        }

        guard let currentID = snapshot.defaultInput, currentID != target.id else { return [] }
        guard let current = snapshot.device(id: currentID) else { return [] }

        let isPinned = current.isListed(in: config.input)
        let isBlocked = current.isListed(in: config.blockedInput)
        guard isPinned || isBlocked else { return [] }

        let cause: String
        if isBlocked {
            cause = "blocked"
        } else if snapshot.liveness[current.uid] == .silent {
            cause = "\(current.name) silent"
        } else {
            cause = "higher priority present"
        }

        return [.setDefaultInput(target.id, reason: "\(current.name) -> \(target.name) (\(cause))")]
    }
}
