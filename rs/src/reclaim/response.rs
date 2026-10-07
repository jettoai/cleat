//! What `audioaccessoryd` answered (Swift `RouteResponse`, `SmartRoutingClient.swift:10-63`). The
//! three fields are kept raw so their meaning is worked out in one pure function.

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RouteResponse {
    /// `1` is Route. Nothing else is documented, and nothing else is treated as success.
    pub action: Option<i64>,
    pub reason: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The hijack went through and the headset is coming back.
    Routed,
    /// It was already here. Nothing changed, so nothing is logged.
    AlreadyRouted,
    /// The phone outscored us. Worth saying once, and worth backing off from.
    HeldByRemote(String),
    /// A previous request of ours is still running; the next beat is soon enough.
    Busy,
    /// Anything else, including an error where the reason is empty.
    Refused(String),
}

pub const ROUTE_ACTION: i64 = 1;

impl RouteResponse {
    pub fn outcome(&self) -> Outcome {
        let text = self.reason.clone().unwrap_or_default();
        let folded = text.to_lowercase();
        // "Already routed" wins over a Route action: the headset was here before we asked.
        if folded.contains("already routed") {
            return Outcome::AlreadyRouted;
        }
        if self.action == Some(ROUTE_ACTION) {
            return Outcome::Routed;
        }
        if folded.contains("remote category") {
            let detail = text.strip_prefix("Rejected, ").unwrap_or(&text);
            return Outcome::HeldByRemote(detail.to_string());
        }
        // "Previous hijack hasn't finished", matched on two words rather than the apostrophe.
        if folded.contains("hijack") && folded.contains("finished") {
            return Outcome::Busy;
        }
        if !text.is_empty() {
            return Outcome::Refused(text);
        }
        Outcome::Refused(self.error.clone().unwrap_or_else(|| "no reason given".into()))
    }
}
