//! `photos::convert` against the synthetic fixtures: size, orientation, format, metadata.

mod common;

use cleat_rs::photos::convert::{finish, to_jpeg};
use common::photos::{fixture, has_gps, jpeg_size, Scratch};

fn convert(name: &str, long_edge: u32, quality: u32) -> (Scratch, Result<(usize, usize), String>) {
    let dir = Scratch::new();
    let result = to_jpeg(&fixture(name), &dir.path("out.jpg"), long_edge, quality);
    (dir, result)
}

#[test]
fn the_longer_side_becomes_long_edge_and_a_tall_picture_stays_tall() {
    let (dir, wide) = convert("wide.heic", 320, 85);
    assert_eq!(wide, Ok((320, 180)));
    let (_dir, tall) = convert("tall.heic", 320, 85);
    // Stored 640x360 with Orientation 6: upright it is portrait, and the pixels must be turned.
    assert_eq!(tall, Ok((180, 320)));
    let bytes = std::fs::read(dir.path("out.jpg")).unwrap();
    assert_eq!(jpeg_size(&bytes), Some((320, 180)));
}

#[test]
fn the_output_is_a_jpeg_whose_pixels_match_what_was_reported() {
    let (dir, result) = convert("tall.heic", 320, 85);
    let bytes = std::fs::read(dir.path("out.jpg")).unwrap();
    assert_eq!(bytes[..2], [0xFF, 0xD8]);
    assert_eq!(jpeg_size(&bytes), result.ok());
    assert_eq!(jpeg_size(&bytes), Some((180, 320)));
}

#[test]
fn zero_keeps_the_original_size_upright() {
    assert_eq!(convert("wide.heic", 0, 85).1, Ok((640, 360)));
    assert_eq!(convert("tall.heic", 0, 85).1, Ok((360, 640)));
}

#[test]
fn the_location_is_dropped_and_the_colour_profile_kept() {
    // The input does carry a location, or the next assertion would hold for any output.
    assert!(has_gps(&fixture("tall.heic")));
    let (dir, result) = convert("tall.heic", 320, 85);
    result.unwrap();
    assert!(!has_gps(&dir.path("out.jpg")));
    let bytes = std::fs::read(dir.path("out.jpg")).unwrap();
    // The profile's name is UTF-16BE inside the ICC block.
    let name: Vec<u8> = "Display P3".encode_utf16().flat_map(u16::to_be_bytes).collect();
    assert!(bytes.windows(name.len()).any(|w| w == name), "the Display P3 profile is not in the file");
}

#[test]
fn quality_changes_the_size() {
    let size = |q| {
        let (dir, r) = convert("wide.heic", 0, q);
        r.unwrap();
        std::fs::metadata(dir.path("out.jpg")).unwrap().len()
    };
    assert!(size(50) < size(100));
}

#[test]
fn a_file_that_is_not_a_picture_fails_and_leaves_nothing() {
    let dir = Scratch::new();
    std::fs::write(dir.path("bad.heic"), (0..5000u32).map(|i| (i * 7919 % 251) as u8).collect::<Vec<u8>>()).unwrap();
    let result = to_jpeg(&dir.path("bad.heic"), &dir.path("out.jpg"), 320, 85);
    assert!(result.is_err(), "{result:?}");
    assert_eq!(dir.names(), ["bad.heic"]);
    assert!(to_jpeg(&dir.path("missing.heic"), &dir.path("out.jpg"), 320, 85).is_err());
}

#[test]
fn finish_names_the_jpeg_and_files_the_original() {
    let dir = Scratch::new();
    std::fs::copy(fixture("wide.heic"), dir.path("IMG_1.HEIC")).unwrap();
    let temp = dir.path(".IMG_1.jpg.cleat-tmp");
    to_jpeg(&dir.path("IMG_1.HEIC"), &temp, 320, 85).unwrap();
    finish(&temp, &dir.path("IMG_1.jpg"), &dir.path("IMG_1.HEIC"), &dir.path("HEIC原檔/IMG_1.HEIC")).unwrap();
    assert_eq!(dir.names(), ["HEIC原檔", "IMG_1.jpg"]);
    assert_eq!(std::fs::read(dir.path("HEIC原檔/IMG_1.HEIC")).unwrap(), std::fs::read(fixture("wide.heic")).unwrap());
}

/// A temporary JPEG and an original in `dir`, ready for `finish`.
fn staged(dir: &Scratch) {
    std::fs::write(dir.path(".IMG.jpg.cleat-tmp"), b"NEW JPEG").unwrap();
    std::fs::write(dir.path("IMG.HEIC"), b"NEW HEIC").unwrap();
}

fn finish_in(dir: &Scratch) -> std::io::Result<(String, String)> {
    finish(&dir.path(".IMG.jpg.cleat-tmp"), &dir.path("IMG.jpg"), &dir.path("IMG.HEIC"), &dir.path("orig/IMG.HEIC"))
}

#[test]
fn finish_never_replaces_a_jpeg_that_arrived_after_the_plan() {
    let dir = Scratch::new();
    staged(&dir);
    std::fs::write(dir.path("IMG.jpg"), b"USER JPEG").unwrap();
    assert_eq!(finish_in(&dir).unwrap(), ("IMG 2.jpg".to_string(), "IMG.HEIC".to_string()));
    assert_eq!(std::fs::read(dir.path("IMG.jpg")).unwrap(), b"USER JPEG");
    assert_eq!(std::fs::read(dir.path("IMG 2.jpg")).unwrap(), b"NEW JPEG");
    assert_eq!(dir.names(), ["IMG 2.jpg", "IMG.jpg", "orig"], "no temporary file, no source left");
}

#[test]
fn finish_never_replaces_an_original_that_arrived_after_the_plan() {
    let dir = Scratch::new();
    staged(&dir);
    std::fs::create_dir(dir.path("orig")).unwrap();
    std::fs::write(dir.path("orig/IMG.HEIC"), b"EARLIER ORIGINAL").unwrap();
    assert_eq!(finish_in(&dir).unwrap(), ("IMG.jpg".to_string(), "IMG 2.HEIC".to_string()));
    assert_eq!(std::fs::read(dir.path("orig/IMG.HEIC")).unwrap(), b"EARLIER ORIGINAL");
    assert_eq!(std::fs::read(dir.path("orig/IMG 2.HEIC")).unwrap(), b"NEW HEIC");
    assert_eq!(dir.names(), ["IMG.jpg", "orig"]);
}

#[test]
fn finish_treats_a_name_differing_only_in_case_as_taken() {
    let dir = Scratch::new();
    staged(&dir);
    std::fs::write(dir.path("img.jpg"), b"USER JPEG").unwrap();
    assert_eq!(finish_in(&dir).unwrap().0, "IMG 2.jpg");
    assert_eq!(std::fs::read(dir.path("img.jpg")).unwrap(), b"USER JPEG");
}

#[test]
fn finish_leaves_a_clean_folder() {
    let dir = Scratch::new();
    staged(&dir);
    assert_eq!(finish_in(&dir).unwrap(), ("IMG.jpg".to_string(), "IMG.HEIC".to_string()));
    assert_eq!(dir.names(), ["IMG.jpg", "orig"]);
}
