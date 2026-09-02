import CoreAudio

/// Rule 4: the same priority list for outputs, without the liveness gate (an output sends no
/// signal to measure) and without a blocked list (nothing hijacks the output the way AirPods
/// hijack the input). A default output that is not on the list was chosen by hand, so it stays.
enum OutputPinRule {

    static func reconcile(_ snapshot: DeviceSnapshot, _ config: Config) -> [Action] {
        guard !config.output.isEmpty else { return [] }

        let candidates = config.output.compactMap { snapshot.device(matching: $0, input: false) }
        guard let target = candidates.first else { return [] }

        guard let currentID = snapshot.defaultOutput, currentID != target.id else { return [] }
        guard let current = snapshot.device(id: currentID) else { return [] }
        guard current.isListed(in: config.output) else { return [] }

        return [.setDefaultOutput(
            target.id,
            reason: "\(current.name) -> \(target.name) (higher priority present)"
        )]
    }
}
