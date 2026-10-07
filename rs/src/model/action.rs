use super::AudioDeviceId;

/// A single write back to CoreAudio, decided by a rule and carried out by the engine. `reason` is
/// the whole log line, composed where the decision was made. `RequestRoute` is the one action that
/// is not a CoreAudio write.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    SetDefaultInput(AudioDeviceId, String),
    SetDefaultOutput(AudioDeviceId, String),
    SetBalance(AudioDeviceId, f32, String),
    SetInputVolume(AudioDeviceId, f32, String),
    RequestRoute { name: String, address: String, reason: String },
}

impl Action {
    pub fn reason(&self) -> &str {
        match self {
            Action::SetDefaultInput(_, r)
            | Action::SetDefaultOutput(_, r)
            | Action::SetBalance(_, _, r)
            | Action::SetInputVolume(_, _, r)
            | Action::RequestRoute { reason: r, .. } => r,
        }
    }

    /// Prefix used in the event log.
    pub fn label(&self) -> &'static str {
        match self {
            Action::SetDefaultInput(..) => "pinInput",
            Action::SetDefaultOutput(..) => "pinOutput",
            Action::SetBalance(..) => "balance",
            Action::SetInputVolume(..) => "inputVolume",
            Action::RequestRoute { .. } => "reclaim",
        }
    }
}
