/// What TCC says about the microphone, as the engine needs it (Swift `MicrophonePermission`).
/// An unanswered dialog is not a refusal: see `Engine::liveness_for_rules`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MicrophonePermission {
    /// The dialog is on screen, or has never been shown.
    Pending,
    Granted,
    /// Denied, restricted, or a case macOS has not told us about, carrying the word `status` prints.
    Denied(String),
}

impl MicrophonePermission {
    pub fn is_granted(&self) -> bool {
        *self == MicrophonePermission::Granted
    }

    pub fn label(&self) -> &str {
        match self {
            MicrophonePermission::Pending => "not determined",
            MicrophonePermission::Granted => "authorized",
            MicrophonePermission::Denied(word) => word,
        }
    }
}
