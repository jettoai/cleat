//! HEIC to JPEG through the system's ImageIO, declared by hand like `display/coregraphics.rs`.
//! The thumbnail call decodes (hardware where there is any), turns the picture upright and scales
//! it; a thumbnail carries no EXIF, so the location goes with the rest of the metadata, while the
//! colour profile stays with the pixels.

use std::path::Path;
use std::ptr::{self, NonNull};

use objc2_core_foundation::{CFBoolean, CFDictionary, CFNumber, CFRetained, CFString, CFType, CFURL};

/// Longer side that stands for "no scaling": the thumbnail call does not enlarge, and with the
/// transform flag it is the only call that also applies the picture's orientation.
const UNSCALED: i32 = 100_000;

#[link(name = "ImageIO", kind = "framework")]
unsafe extern "C" {
    fn CGImageSourceCreateWithURL(url: &CFURL, options: *const CFDictionary) -> *const CFType;
    fn CGImageSourceCreateThumbnailAtIndex(source: &CFType, index: usize, options: &CFDictionary) -> *const CFType;
    fn CGImageDestinationCreateWithURL(
        url: &CFURL,
        kind: &CFString,
        count: usize,
        options: *const CFDictionary,
    ) -> *const CFType;
    fn CGImageDestinationAddImage(destination: &CFType, image: &CFType, properties: &CFDictionary);
    fn CGImageDestinationFinalize(destination: &CFType) -> bool;
    static kCGImageSourceCreateThumbnailFromImageAlways: &'static CFString;
    static kCGImageSourceCreateThumbnailWithTransform: &'static CFString;
    static kCGImageSourceThumbnailMaxPixelSize: &'static CFString;
    static kCGImageDestinationLossyCompressionQuality: &'static CFString;
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGImageGetWidth(image: &CFType) -> usize;
    fn CGImageGetHeight(image: &CFType) -> usize;
}

/// Takes the +1 reference a Create function returns.
fn owned(raw: *const CFType) -> Option<CFRetained<CFType>> {
    // SAFETY: the Create rule hands over one retain, and null means failure.
    NonNull::new(raw.cast_mut()).map(|p| unsafe { CFRetained::from_raw(p) })
}

fn dictionary(pairs: &[(&CFString, &CFType)]) -> CFRetained<CFDictionary<CFString, CFType>> {
    let keys: Vec<&CFString> = pairs.iter().map(|(k, _)| *k).collect();
    let values: Vec<&CFType> = pairs.iter().map(|(_, v)| *v).collect();
    CFDictionary::from_slices(&keys, &values)
}

/// Writes `src` as a JPEG at `dst`: the longer side `long_edge` pixels (0 keeps the size), quality
/// in percent. Returns the pixel size written. A failure leaves nothing at `dst`.
pub fn to_jpeg(src: &Path, dst: &Path, long_edge: u32, quality: u32) -> Result<(usize, usize), String> {
    let result = write_jpeg(src, dst, long_edge, quality);
    if result.is_err() {
        let _ = std::fs::remove_file(dst);
    }
    result
}

fn write_jpeg(src: &Path, dst: &Path, long_edge: u32, quality: u32) -> Result<(usize, usize), String> {
    let src_url = CFURL::from_file_path(src).ok_or("path is not a file URL")?;
    let dst_url = CFURL::from_file_path(dst).ok_or("path is not a file URL")?;
    // SAFETY: valid URL, null options.
    let source = owned(unsafe { CGImageSourceCreateWithURL(&src_url, ptr::null()) }).ok_or("cannot open the file")?;
    let max_pixels = CFNumber::new_i32(if long_edge == 0 { UNSCALED } else { long_edge as i32 });
    let on = CFBoolean::new(true);
    // SAFETY: the three keys are ImageIO's own constants.
    let options = unsafe {
        dictionary(&[
            (kCGImageSourceCreateThumbnailFromImageAlways, on.as_ref()),
            (kCGImageSourceCreateThumbnailWithTransform, on.as_ref()),
            (kCGImageSourceThumbnailMaxPixelSize, max_pixels.as_ref()),
        ])
    };
    // SAFETY: `source` is a live image source and `options` a live dictionary.
    let image = owned(unsafe { CGImageSourceCreateThumbnailAtIndex(&source, 0, options.as_ref()) })
        .ok_or("cannot decode the picture")?;
    let jpeg = CFString::from_static_str("public.jpeg");
    // SAFETY: valid URL and type; one image is added below, as the count says.
    let destination = owned(unsafe { CGImageDestinationCreateWithURL(&dst_url, &jpeg, 1, ptr::null()) })
        .ok_or("cannot create the output file")?;
    let quality = CFNumber::new_f64(f64::from(quality) / 100.0);
    // SAFETY: the key is ImageIO's own constant.
    let properties = unsafe { dictionary(&[(kCGImageDestinationLossyCompressionQuality, quality.as_ref())]) };
    // SAFETY: `destination` and `image` are live, finalised once.
    let (written, size) = unsafe {
        CGImageDestinationAddImage(&destination, &image, properties.as_ref());
        (CGImageDestinationFinalize(&destination), (CGImageGetWidth(&image), CGImageGetHeight(&image)))
    };
    if written { Ok(size) } else { Err("cannot write the JPEG".into()) }
}

/// Renames the finished temporary file to its final name, then moves the original into
/// `originals_dir` (made if need be).
/// ponytail: `rename` stays on one volume; an originals folder on another disk fails and is
/// reported, add a copy-and-delete fallback if that is ever asked for.
pub fn finish(temp: &Path, jpeg: &Path, original: &Path, original_dst: &Path) -> std::io::Result<()> {
    std::fs::rename(temp, jpeg)?;
    if let Some(dir) = original_dst.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::rename(original, original_dst)
}
