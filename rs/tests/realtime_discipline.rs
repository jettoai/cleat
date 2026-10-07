//! Proof two of three (plan §4.2-2): the realtime span of `src/liveness/ioproc.rs`, between the
//! REALTIME markers, names nothing that allocates, locks or dispatches, and the file carries the
//! clippy deny list that rules out panics and unchecked arithmetic.

const SOURCE: &str = include_str!("../src/liveness/ioproc.rs");

const FORBIDDEN: &[&str] = &[
    "Box", "Vec", "String", "format!", "println", "eprintln", "to_string", "to_owned", "clone()", "Mutex",
    "RwLock", "lock(", "send(", "Arc::", "Rc::", "SystemTime", "Instant", "note(", "async", "dispatch_async",
    "exec_async",
];

fn realtime_span() -> &'static str {
    let begin = SOURCE.find("// REALTIME BEGIN").expect("BEGIN marker");
    let end = SOURCE.find("// REALTIME END").expect("END marker");
    assert!(begin < end);
    &SOURCE[begin..end]
}

#[test]
fn realtime_span_names_nothing_that_allocates_locks_or_dispatches() {
    let span = realtime_span();
    assert!(span.contains("fn io_proc") && span.contains("fn consume") && span.contains("merge_data(1)"));
    let found: Vec<&&str> = FORBIDDEN.iter().filter(|w| span.contains(**w)).collect();
    assert!(found.is_empty(), "forbidden in the realtime span: {found:?}");
}

#[test]
fn ioproc_file_denies_panics_and_unchecked_arithmetic() {
    let head = &SOURCE[..SOURCE.find("// REALTIME BEGIN").unwrap()];
    for lint in [
        "clippy::indexing_slicing",
        "clippy::panic",
        "clippy::unwrap_used",
        "clippy::expect_used",
        "clippy::integer_division",
        "clippy::arithmetic_side_effects",
    ] {
        assert!(head.contains(lint), "missing deny {lint}");
    }
    assert!(head.contains("#![deny("));
}
