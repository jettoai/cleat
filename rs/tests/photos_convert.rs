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
