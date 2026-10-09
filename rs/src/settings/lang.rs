//! Which language the window and the menu speak: Traditional Chinese when the system's first
//! preferred language is it, English otherwise. Decided once per process.

use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    ZhHant,
    En,
}

/// Pure: the first entry of `NSLocale.preferredLanguages` decides.
pub fn from_preferred(preferred: &[String]) -> Lang {
    let Some(first) = preferred.first() else { return Lang::En };
    let tag = first.to_ascii_lowercase().replace('_', "-");
    let zh_hant = ["zh-hant", "zh-tw", "zh-hk", "zh-mo"]
        .iter()
        .any(|p| tag == *p || tag.starts_with(&format!("{p}-")));
    if zh_hant { Lang::ZhHant } else { Lang::En }
}

/// The process's language. `-AppleLanguages '(en)'` on the command line reaches it through
/// NSUserDefaults' argument domain.
pub fn current() -> Lang {
    static LANG: OnceLock<Lang> = OnceLock::new();
    *LANG.get_or_init(|| {
        let langs: Vec<String> = objc2_foundation::NSLocale::preferredLanguages().iter().map(|s| s.to_string()).collect();
        from_preferred(&langs)
    })
}
