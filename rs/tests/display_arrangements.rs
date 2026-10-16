//! The memory file (Swift `DisplayArrangementStoreTests`), case for case.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use cleat_rs::state::arrangements::{read, write, Arrangements, Loaded};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

struct Dir(PathBuf);

impl Dir {
    fn new() -> Self {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("cleat-rs-arrangements-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
    fn file(&self) -> PathBuf {
        self.0.join("display-arrangements.json")
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn arrangements(entries: &[(&str, &[(&str, [i32; 2])])]) -> Arrangements {
    entries
        .iter()
        .map(|(key, places)| (key.to_string(), places.iter().map(|(u, p)| (u.to_string(), *p)).collect()))
        .collect()
}

#[test]
fn a_missing_file_is_empty() {
    assert_eq!(read(&Dir::new().file()), Loaded::Empty);
}

#[test]
fn malformed_json_is_unreadable() {
    let d = Dir::new();
    std::fs::write(d.file(), "{not json").unwrap();
    assert!(matches!(read(&d.file()), Loaded::Unreadable(_)));
}

#[test]
fn entries_that_are_not_a_pair_are_dropped() {
    let d = Dir::new();
    std::fs::write(d.file(), r#"{"k":{"u":[1],"v":[1,2]},"gone":{"w":[1,2,3]}}"#).unwrap();
    assert_eq!(read(&d.file()), Loaded::Loaded(arrangements(&[("k", &[("v", [1, 2])])])));
}

#[test]
fn write_then_read_round_trips() {
    let d = Dir::new();
    let a = arrangements(&[("a|b", &[("a", [0, 0]), ("b", [10, -5])]), ("c", &[("c", [0, 0])])]);
    assert!(write(&a, &d.file()));
    assert_eq!(read(&d.file()), Loaded::Loaded(a));
    let text = std::fs::read_to_string(d.file()).unwrap();
    assert!(text.find("a|b").unwrap() < text.find("\"c\"").unwrap(), "keys are sorted");
}

#[test]
fn write_leaves_no_temporary_files() {
    let d = Dir::new();
    for i in 0..3 {
        assert!(write(&arrangements(&[("k", &[("u", [i, 0])])]), &d.file()));
    }
    let names: Vec<String> =
        std::fs::read_dir(&d.0).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(names, vec!["display-arrangements.json".to_string()]);
}

#[test]
fn write_creates_the_directory() {
    let d = Dir::new();
    let nested = d.0.join("a/b/display-arrangements.json");
    let a = arrangements(&[("k", &[("u", [1, 2])])]);
    assert!(write(&a, &nested));
    assert_eq!(read(&nested), Loaded::Loaded(a));
}
