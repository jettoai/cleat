//! Matching a config entry against a device. Names are compared after normalisation (device
//! names carry no-break spaces that copy as plain spaces), UIDs literally.

use unicode_normalization::UnicodeNormalization;

/// Space characters that render as a plain space but are not one.
const SPACE_LIKE: [char; 5] = ['\u{00A0}', '\u{202F}', '\u{2007}', '\u{2009}', '\u{200A}'];

/// NFKC, then every space-like character to a plain space, then trim and collapse runs.
/// Case is preserved: "brio 100" is a typo, not a match.
pub fn normalize(value: &str) -> String {
    let mapped: String = value
        .nfkc()
        .map(|c| if SPACE_LIKE.contains(&c) { ' ' } else { c })
        .collect();
    mapped.split(char::is_whitespace).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" ")
}

/// True when a config entry names this device, either by normalised name or by exact UID.
pub fn matches(entry: &str, name: &str, uid: &str) -> bool {
    if !entry.is_empty() && entry == uid {
        return true;
    }
    normalize(entry) == normalize(name)
}
