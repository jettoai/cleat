//! Which builds are development builds (plan D2): `.dev`, `.rs` and no bundle at all.

use cleat_rs::identity::Identity;

fn id(bundle: Option<&str>) -> Identity {
    Identity { bundle_id: bundle.map(String::from), version: None }
}

#[test]
fn only_the_release_bundle_id_is_not_a_development_build() {
    assert!(!id(Some("ai.jetto.cleat")).is_development_build());
    assert!(id(Some("ai.jetto.cleat.dev")).is_development_build());
    assert!(id(Some("ai.jetto.cleat.rs")).is_development_build());
    assert!(id(None).is_development_build());
}

#[test]
fn label_and_version_fall_back_for_a_bare_executable() {
    assert_eq!(id(None).label(), "ai.jetto.cleat.rs.dev");
    assert_eq!(id(None).version(), env!("CARGO_PKG_VERSION"));
    assert_eq!(Identity::current(), Identity::default());
}
