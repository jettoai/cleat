import CoreAudio

/// A single write back to CoreAudio, decided by a rule and carried out by the engine.
///
/// `reason` is the whole log line, composed where the decision was made - the engine logs it
/// verbatim rather than re-deriving why it is about to do this, which is how the log and the
/// decision drift apart.
///
/// `requestRoute` is the one exception, and the only action that is not a CoreAudio write: it asks
/// `audioaccessoryd` to route a headset back here and is answered asynchronously, so what happened
/// is not known when the action is made. Its `reason` travels to the daemon as the request's own
/// reason string, and the log line is composed from the answer instead.
enum Action: Equatable, Sendable {
    case setDefaultInput(AudioDeviceID, reason: String)
    case setDefaultOutput(AudioDeviceID, reason: String)
    case setBalance(AudioDeviceID, Float, reason: String)
    case setInputVolume(AudioDeviceID, Float, reason: String)
    case requestRoute(name: String, address: String, reason: String)

    var reason: String {
        switch self {
        case .setDefaultInput(_, let reason),
             .setDefaultOutput(_, let reason),
             .setBalance(_, _, let reason),
             .setInputVolume(_, _, let reason),
             .requestRoute(_, _, let reason):
            return reason
        }
    }

    /// Prefix used in the event log, so `cleat log` reads as one column of rule names.
    var label: String {
        switch self {
        case .setDefaultInput: return "pinInput"
        case .setDefaultOutput: return "pinOutput"
        case .setBalance: return "balance"
        case .setInputVolume: return "inputVolume"
        case .requestRoute: return "reclaim"
        }
    }
}
