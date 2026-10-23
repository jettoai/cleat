//! The HEIC worker thread against a real folder: kqueue, debounce, settle check, retries.
//! Waits are injected small; a folder change is a real one.

mod common;

use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use cleat_rs::config::HeicConfig;
use cleat_rs::engine::Event;
use cleat_rs::photos::{self, Handle, Report, Timing};
use common::photos::{fixture, Scratch};

const FAST: Timing = Timing {
    debounce: Duration::from_millis(60),
    stable: Duration::from_millis(60),
    retries: [Duration::from_millis(40); 3],
};

fn watch(dir: &Scratch, timing: Timing) -> (Handle, Receiver<Event>) {
    let config = HeicConfig {
        watch: dir.0.display().to_string(),
        originals: "HEIC原檔".into(),
        long_edge: 320,
        quality: 85,
    };
    let (tx, rx) = mpsc::channel();
    let handle = photos::start(&config, std::path::Path::new("/nonexistent-home"), tx, timing).expect("the watch starts");
    (handle, rx)
}

/// A new file, born now: `fs::copy` would carry the fixture's own birth time over.
fn place(fixture_name: &str, to: &std::path::Path) {
    std::fs::write(to, std::fs::read(fixture(fixture_name)).unwrap()).unwrap();
}

fn next(rx: &Receiver<Event>, within: Duration) -> Option<Report> {
    match rx.recv_timeout(within) {
        Ok(Event::Photos(r)) => Some(r),
        Ok(_) => panic!("the worker sends only photos reports"),
        Err(_) => None,
    }
}

fn expect(rx: &Receiver<Event>) -> Report {
    next(rx, Duration::from_secs(10)).expect("a report within 10 seconds")
}

#[test]
fn a_new_heic_becomes_a_jpeg_and_its_original_is_filed() {
    let dir = Scratch::new();
    // Already there when the watch starts: left alone.
    place("wide.heic", &dir.path("OLD.HEIC"));
    std::thread::sleep(Duration::from_millis(30));
    let (_handle, rx) = watch(&dir, FAST);
    let began = Instant::now();
    place("tall.heic", &dir.path("IMG_5708.HEIC"));

    let report = expect(&rx);
    let took = began.elapsed();
    let Report::Converted { from, to, width, height, ms, originals } = &report else { panic!("{report:?}") };
    assert_eq!((from.as_str(), to.as_str(), *width, *height), ("IMG_5708.HEIC", "IMG_5708.jpg", 180, 320));
    assert_eq!(originals, "HEIC原檔");
    assert_eq!(report.line(), format!("heic: IMG_5708.HEIC -> IMG_5708.jpg 180x320 {ms} ms, original -> HEIC原檔/"));
    assert_eq!(dir.names(), ["HEIC原檔", "IMG_5708.jpg", "OLD.HEIC"], "no temporary file is left");
    assert_eq!(
        std::fs::read(dir.path("HEIC原檔/IMG_5708.HEIC")).unwrap(),
        std::fs::read(fixture("tall.heic")).unwrap()
    );
    eprintln!("arrived to reported in {took:?}, conversion {ms} ms");
    // The move and the new file stir the folder again; nothing more may come of it.
    assert_eq!(next(&rx, Duration::from_millis(500)), None);
}

#[test]
fn a_copy_that_keeps_the_source_birth_time_is_still_new() {
    let dir = Scratch::new();
    place("wide.heic", &dir.path("BEFORE.HEIC"));
    std::thread::sleep(Duration::from_millis(30));
    let (_handle, rx) = watch(&dir, FAST);
    // `fs::copy` carries the birth time of tests/fixtures/wide.heic, long before the watch began.
    std::fs::copy(fixture("wide.heic"), dir.path("IMG_7.HEIC")).unwrap();
    let born = std::fs::metadata(dir.path("IMG_7.HEIC")).unwrap().created().unwrap();
    assert!(born < std::time::SystemTime::now() - Duration::from_secs(1), "the copy has an old birth time: {born:?}");

    let Report::Converted { from, .. } = expect(&rx) else { panic!("not converted") };
    assert_eq!(from, "IMG_7.HEIC");
    assert_eq!(dir.names(), ["BEFORE.HEIC", "HEIC原檔", "IMG_7.jpg"], "what was there before the watch stays");
}

#[test]
fn a_name_used_again_after_conversion_is_converted_again() {
    let dir = Scratch::new();
    let (_handle, rx) = watch(&dir, FAST);
    place("wide.heic", &dir.path("IMG.HEIC"));
    let Report::Converted { to, .. } = expect(&rx) else { panic!("not converted") };
    assert_eq!(to, "IMG.jpg");
    assert_eq!(next(&rx, Duration::from_millis(300)), None);
    // The same name, the same size, a moment later: AirDrop reuses names.
    place("wide.heic", &dir.path("IMG.HEIC"));
    let Report::Converted { from, to, .. } = expect(&rx) else { panic!("not converted the second time") };
    assert_eq!((from.as_str(), to.as_str()), ("IMG.HEIC", "IMG 2.jpg"));
    assert_eq!(dir.names(), ["HEIC原檔", "IMG 2.jpg", "IMG.jpg"]);
    assert_eq!(std::fs::read_dir(dir.path("HEIC原檔")).unwrap().count(), 2);
    assert!(dir.path("HEIC原檔/IMG 2.HEIC").exists() && dir.path("HEIC原檔/IMG.HEIC").exists());
}

#[test]
fn names_that_are_taken_get_a_number_in_both_folders() {
    let dir = Scratch::new();
    std::fs::create_dir(dir.path("HEIC原檔")).unwrap();
    std::fs::write(dir.path("HEIC原檔/IMG_1.HEIC"), b"an earlier original").unwrap();
    std::fs::write(dir.path("IMG_1.jpg"), b"someone else's jpeg").unwrap();
    let (_handle, rx) = watch(&dir, FAST);
    place("wide.heic", &dir.path("IMG_1.HEIC"));

    let Report::Converted { to, .. } = expect(&rx) else { panic!("not converted") };
    assert_eq!(to, "IMG_1 2.jpg");
    assert_eq!(std::fs::read(dir.path("IMG_1.jpg")).unwrap(), b"someone else's jpeg");
    assert_eq!(std::fs::read(dir.path("HEIC原檔/IMG_1.HEIC")).unwrap(), b"an earlier original");
    assert_eq!(std::fs::read(dir.path("HEIC原檔/IMG_1 2.HEIC")).unwrap(), std::fs::read(fixture("wide.heic")).unwrap());
}

#[test]
fn a_file_still_being_written_is_converted_once_it_is_whole() {
    use std::io::Write;
    let dir = Scratch::new();
    let timing = Timing { debounce: Duration::from_millis(60), stable: Duration::from_millis(250), ..FAST };
    let (_handle, rx) = watch(&dir, timing);
    let whole = std::fs::read(fixture("wide.heic")).unwrap();
    let thirds = [&whole[..whole.len() / 3], &whole[whole.len() / 3..whole.len() * 2 / 3], &whole[whole.len() * 2 / 3..]];
    let path = dir.path("IMG_2.HEIC");
    std::fs::write(&path, thirds[0]).unwrap();
    // The worker lists the folder ~60 ms from now and then checks the size every 250 ms: the
    // second third lands before the first check, the last one before the second.
    let writer = {
        let (path, rest) = (path.clone(), [thirds[1].to_vec(), thirds[2].to_vec()]);
        std::thread::spawn(move || {
            for (wait, chunk) in [150, 300].into_iter().zip(rest) {
                std::thread::sleep(Duration::from_millis(wait));
                std::fs::OpenOptions::new().append(true).open(&path).unwrap().write_all(&chunk).unwrap();
            }
        })
    };
    let report = expect(&rx);
    writer.join().unwrap();
    assert!(matches!(report, Report::Converted { width: 320, height: 180, .. }), "{report:?}");
    assert_eq!(dir.names(), ["HEIC原檔", "IMG_2.jpg"]);
}

#[test]
fn a_file_that_will_not_decode_is_retried_three_times_then_left_in_place() {
    let dir = Scratch::new();
    let timing = Timing { retries: [Duration::from_millis(150); 3], ..FAST };
    let (_handle, rx) = watch(&dir, timing);
    let began = Instant::now();
    std::fs::write(dir.path("BAD.HEIC"), (0..5000u32).map(|i| (i * 7919 % 251) as u8).collect::<Vec<u8>>()).unwrap();

    let report = expect(&rx);
    let Report::Failed { name, reason } = &report else { panic!("{report:?}") };
    assert_eq!(name, "BAD.HEIC");
    assert!(reason.ends_with("left in place after 3 retries"), "{reason}");
    assert!(began.elapsed() >= Duration::from_millis(450), "three waits of 150 ms came first: {:?}", began.elapsed());
    assert_eq!(dir.names(), ["BAD.HEIC"]);
    // Not tried again on the folder's next stir.
    std::fs::write(dir.path("other.txt"), b"x").unwrap();
    assert_eq!(next(&rx, Duration::from_millis(600)), None);
}

#[test]
fn an_unreadable_folder_is_reported_once_and_the_watch_recovers() {
    use std::os::unix::fs::PermissionsExt;
    let dir = Scratch::new();
    std::fs::set_permissions(&dir.0, std::fs::Permissions::from_mode(0o000)).unwrap();
    let (_handle, rx) = watch(&dir, FAST);

    let report = expect(&rx);
    assert!(matches!(&report, Report::NoAccess { .. }), "{report:?}");
    assert!(report.line().starts_with("heic: no access to "), "{}", report.line());
    // The folder is retried every 2 seconds; the same reason is not said twice.
    assert_eq!(next(&rx, Duration::from_millis(2600)), None);

    std::fs::set_permissions(&dir.0, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::thread::sleep(Duration::from_millis(2600)); // the next retry opens it
    let back = expect(&rx);
    assert!(matches!(&back, Report::Readable { .. }), "{back:?}");
    assert!(back.line().ends_with(" can be read again"), "{}", back.line());
    place("wide.heic", &dir.path("IMG_3.HEIC"));
    assert!(matches!(expect(&rx), Report::Converted { .. }));
}

#[test]
fn a_folder_that_appears_later_is_picked_up_with_what_it_holds() {
    let dir = Scratch::new();
    let inbox = dir.path("inbox");
    let config = HeicConfig { watch: inbox.display().to_string(), long_edge: 320, ..HeicConfig::default() };
    let (tx, rx) = mpsc::channel();
    let _handle = photos::start(&config, std::path::Path::new("/nonexistent-home"), tx, FAST).unwrap();
    // The folder is missing: said once, and the watch keeps looking (every 2 s) instead of giving up.
    assert!(matches!(expect(&rx), Report::Failed { .. }));
    // It arrives whole, with a new file in it (born after the watch started).
    std::fs::create_dir(dir.path("prep")).unwrap();
    place("wide.heic", &dir.path("prep/IMG_6.HEIC"));
    std::fs::rename(dir.path("prep"), &inbox).unwrap();
    assert!(matches!(expect(&rx), Report::Converted { .. }));
    assert!(inbox.join("IMG_6.jpg").exists());
}

#[test]
fn dropping_the_handle_ends_the_watch() {
    let dir = Scratch::new();
    let (handle, rx) = watch(&dir, FAST);
    drop(handle);
    place("wide.heic", &dir.path("IMG_4.HEIC"));
    assert_eq!(next(&rx, Duration::from_millis(500)), None);
    assert_eq!(dir.names(), ["IMG_4.HEIC"]);
}
