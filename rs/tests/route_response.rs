//! Swift `ReclaimTests.testOutcomesAreReadFromTheResponse`: the five answers seen on the wire.

use cleat_rs::reclaim::{Outcome, RouteResponse};

fn r(action: i64, reason: &str) -> RouteResponse {
    RouteResponse { action: Some(action), reason: Some(reason.into()), error: None }
}

#[test]
fn outcomes_are_read_from_the_response() {
    assert_eq!(r(1, "Tipi device hijack was successful").outcome(), Outcome::Routed);
    assert_eq!(r(0, "Device already routed").outcome(), Outcome::AlreadyRouted);
    assert_eq!(r(1, "already routed, wxInfo NULL").outcome(), Outcome::AlreadyRouted);
    assert_eq!(
        r(0, "Rejected, Remote Category 301 > Local Category 200, audio streaming").outcome(),
        Outcome::HeldByRemote("Remote Category 301 > Local Category 200, audio streaming".into())
    );
    assert_eq!(r(0, "Previous hijack hasn't finished").outcome(), Outcome::Busy);
    assert_eq!(
        r(0, "Something new in a later macOS").outcome(),
        Outcome::Refused("Something new in a later macOS".into())
    );
    assert_eq!(
        RouteResponse { action: Some(0), reason: None, error: Some("code 5".into()) }.outcome(),
        Outcome::Refused("code 5".into())
    );
    assert_eq!(RouteResponse::default().outcome(), Outcome::Refused("no reason given".into()));
}
