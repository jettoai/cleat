//! Helpers for the HEIC tests: scratch folders, the synthetic fixtures, and reading a JPEG back.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use objc2_core_foundation::{CFDictionary, CFRetained, CFString, CFType, CFURL};

/// `wide.heic`: 640x360. `tall.heic`: 640x360 stored with Orientation 6, so it shows as 360x640.
/// Both carry GPS and a Display P3 profile (`make-heic.swift`).
pub fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// An empty folder, removed (and made writable again) on drop.
pub struct Scratch(pub PathBuf);

impl Scratch {
    pub fn new() -> Self {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("cleat-photos-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    pub fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    pub fn names(&self) -> Vec<String> {
        let mut v: Vec<String> =
            std::fs::read_dir(&self.0).unwrap().flatten().map(|e| e.file_name().into_string().unwrap()).collect();
        v.sort();
        v
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o755));
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Width and height from the first start-of-frame marker; None when `bytes` is not a JPEG.
pub fn jpeg_size(bytes: &[u8]) -> Option<(usize, usize)> {
    if bytes.get(..2)? != [0xFF, 0xD8] {
        return None;
    }
    let mut i = 2;
    while i + 9 < bytes.len() {
        if bytes[i] != 0xFF {
            return None;
        }
        let marker = bytes[i + 1];
        let len = usize::from(bytes[i + 2]) << 8 | usize::from(bytes[i + 3]);
        if matches!(marker, 0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF) {
            let h = usize::from(bytes[i + 5]) << 8 | usize::from(bytes[i + 6]);
            let w = usize::from(bytes[i + 7]) << 8 | usize::from(bytes[i + 8]);
            return Some((w, h));
        }
        i += 2 + len;
    }
    None
}

#[link(name = "ImageIO", kind = "framework")]
unsafe extern "C" {
    fn CGImageSourceCreateWithURL(url: &CFURL, options: *const CFDictionary) -> *const CFType;
    fn CGImageSourceCopyPropertiesAtIndex(source: &CFType, index: usize, options: *const CFDictionary) -> *const CFDictionary<CFString, CFType>;
    static kCGImagePropertyGPSDictionary: &'static CFString;
}

/// Whether ImageIO reads a GPS block in the file's metadata.
pub fn has_gps(path: &Path) -> bool {
    let url = CFURL::from_file_path(path).unwrap();
    // SAFETY: Create/Copy rule: both results are +1 or null, and are released by `CFRetained`.
    unsafe {
        let source = CGImageSourceCreateWithURL(&url, std::ptr::null());
        let source = CFRetained::from_raw(std::ptr::NonNull::new(source.cast_mut()).expect("readable image"));
        let props = CGImageSourceCopyPropertiesAtIndex(&source, 0, std::ptr::null());
        let props = CFRetained::from_raw(std::ptr::NonNull::new(props.cast_mut()).expect("properties"));
        props.contains_key(kCGImagePropertyGPSDictionary)
    }
}
