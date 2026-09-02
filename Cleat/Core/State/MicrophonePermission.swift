/// What TCC says about the microphone, as the engine needs it.
///
/// Three cases rather than a Bool, because an unanswered dialog is not a refusal. A refusal is a
/// decision that silence detection is off, and the input rule goes back to behaving as it did
/// before liveness existed. An unanswered dialog means nothing has been measured *yet* - a
/// configured receiver might be sending silence and nobody has looked - which is a different thing
/// to tell the rules (see `Engine.livenessForRules`).
enum MicrophonePermission: Equatable, Sendable {

    /// The dialog is on screen, or has never been shown.
    case pending
    case granted
    /// Denied, restricted, or a case macOS has not told us about yet, carrying the word that
    /// `cleat status` prints.
    case denied(String)

    var isGranted: Bool { self == .granted }

    /// The one word `cleat status` and the event log show.
    var label: String {
        switch self {
        case .pending: return "not determined"
        case .granted: return "authorized"
        case .denied(let word): return word
        }
    }
}
