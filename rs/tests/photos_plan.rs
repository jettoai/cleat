//! `photos::plan`: which items of the watched folder become jobs, and under what names.

use std::collections::HashSet;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use cleat_rs::photos::plan::{plan, Entry, Job, Plan, Skip};

fn at(secs: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(secs)
}

const STARTED: u64 = 1_000;

fn file(name: &str, born: u64) -> Entry {
    Entry { name: name.into(), size: 100, born: at(born), is_dir: false, in_baseline: true }
}

fn names(list: &[&str]) -> HashSet<String> {
    list.iter().map(|s| s.to_string()).collect()
}

fn run(entries: &[Entry], existing: &[&str], originals: &[&str]) -> Plan {
    plan(entries, at(STARTED), &names(existing), &names(originals))
}

fn job(source: &str, output: &str, original: &str) -> Job {
    Job { source: source.into(), size: 100, output: output.into(), original: original.into() }
}

#[test]
fn heic_and_heif_in_any_case_become_jpeg_jobs() {
    let entries: Vec<Entry> = ["a.heic", "b.HEIC", "c.Heic", "d.heif", "e.HEIF"].iter().map(|n| file(n, 2000)).collect();
    let p = run(&entries, &[], &[]);
    assert_eq!(
        p.jobs,
        vec![
            job("a.heic", "a.jpg", "a.heic"),
            job("b.HEIC", "b.jpg", "b.HEIC"),
            job("c.Heic", "c.jpg", "c.Heic"),
            job("d.heif", "d.jpg", "d.heif"),
            job("e.HEIF", "e.jpg", "e.HEIF"),
        ]
    );
    assert!(p.skipped.is_empty());
}

#[test]
fn other_files_are_not_candidates_and_not_even_skipped() {
    let entries: Vec<Entry> = ["a.jpg", "b.png", "heic", "c.heic.zip", "d.mov", ".heic"].iter().map(|n| file(n, 2000)).collect();
    assert_eq!(run(&entries, &[], &[]), Plan::default());
}

#[test]
fn a_file_born_before_the_watch_started_is_left_alone() {
    let entries = [file("old.heic", STARTED - 1), file("new.heic", STARTED), file("newer.heic", STARTED + 1)];
    let p = run(&entries, &[], &[]);
    assert_eq!(p.jobs.iter().map(|j| j.source.as_str()).collect::<Vec<_>>(), ["new.heic", "newer.heic"]);
    assert_eq!(p.skipped, vec![("old.heic".to_string(), Skip::Older)]);
}

/// A name that was not in the folder when the watch started.
fn fresh(name: &str, born: u64) -> Entry {
    Entry { in_baseline: false, ..file(name, born) }
}

#[test]
fn only_a_name_that_was_already_there_can_be_older() {
    let entries = [
        fresh("copied.heic", 1),             // not there at the start, born long ago (a copy keeps the source's time)
        file("before.heic", STARTED - 1),    // there at the start, born before it
        file("replaced.heic", STARTED + 1),  // there at the start, but born after it: a new picture under an old name
    ];
    let p = run(&entries, &[], &[]);
    assert_eq!(p.jobs.iter().map(|j| j.source.as_str()).collect::<Vec<_>>(), ["copied.heic", "replaced.heic"]);
    assert_eq!(p.skipped, vec![("before.heic".to_string(), Skip::Older)]);
}

#[test]
fn hidden_and_temporary_names_are_skipped() {
    let entries = [file(".IMG_1.heic", 2000), file(".IMG_1.HEIC.sb-1234-abc", 2000), file(".hidden.heif", 2000)];
    let p = run(&entries, &[], &[]);
    assert!(p.jobs.is_empty());
    assert_eq!(
        p.skipped,
        vec![(".IMG_1.heic".to_string(), Skip::Hidden), (".hidden.heif".to_string(), Skip::Hidden)]
    );
}

#[test]
fn a_folder_named_like_a_heic_is_skipped() {
    let entries = [Entry { is_dir: true, ..file("album.heic", 2000) }];
    let p = run(&entries, &[], &[]);
    assert!(p.jobs.is_empty());
    assert_eq!(p.skipped, vec![("album.heic".to_string(), Skip::Folder)]);
}

#[test]
fn a_taken_output_name_gets_2_then_3_and_is_never_overwritten() {
    let entries = [file("IMG_1.HEIC", 2000)];
    assert_eq!(run(&entries, &["IMG_1.jpg"], &[]).jobs[0].output, "IMG_1 2.jpg");
    assert_eq!(run(&entries, &["IMG_1.jpg", "IMG_1 2.jpg"], &[]).jobs[0].output, "IMG_1 3.jpg");
    // The volume does not tell cases apart, so neither does the plan.
    assert_eq!(run(&entries, &["img_1.JPG"], &[]).jobs[0].output, "IMG_1 2.jpg");
}

#[test]
fn a_taken_name_in_the_originals_folder_is_numbered_the_same_way() {
    let entries = [file("IMG_1.HEIC", 2000)];
    let p = run(&entries, &[], &["IMG_1.HEIC", "IMG_1 2.HEIC"]);
    assert_eq!(p.jobs, vec![job("IMG_1.HEIC", "IMG_1.jpg", "IMG_1 3.HEIC")]);
}

#[test]
fn two_new_files_that_would_share_a_jpeg_name_do_not_collide() {
    // `a.heic` and `a.heif` both want `a.jpg`; the older is served first.
    let entries = [file("a.heif", 2001), file("a.heic", 2000)];
    let p = run(&entries, &[], &[]);
    assert_eq!(p.jobs, vec![job("a.heic", "a.jpg", "a.heic"), job("a.heif", "a 2.jpg", "a.heif")]);
}

#[test]
fn jobs_come_oldest_first_and_a_dotted_stem_keeps_its_dots() {
    let entries = [file("late.heic", 3000), file("my.photo.v2.heic", 2000)];
    let p = run(&entries, &[], &[]);
    assert_eq!(p.jobs.iter().map(|j| j.output.as_str()).collect::<Vec<_>>(), ["my.photo.v2.jpg", "late.jpg"]);
}
