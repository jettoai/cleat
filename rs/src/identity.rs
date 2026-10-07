//! Which build this is, read from the bundle's Info.plist. A bare executable (cargo run, cargo
//! test) has no bundle and counts as a development build.

use objc2_core_foundation::{CFBundle, CFRetained, CFString};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Identity {
    pub bundle_id: Option<String>,
    pub version: Option<String>,
}

fn info_string(bundle: &CFBundle, key: &str) -> Option<String> {
    let key = CFString::from_str(key);
    let value = bundle.value_for_info_dictionary_key(Some(&key))?;
    let s: CFRetained<CFString> = value.downcast().ok()?;
    Some(s.to_string())
}

impl Identity {
    pub fn current() -> Self {
        let Some(bundle) = CFBundle::main_bundle() else { return Self::default() };
        Self {
            bundle_id: bundle.identifier().map(|s| s.to_string()),
            version: info_string(&bundle, "CFBundleShortVersionString"),
        }
    }

    /// The launchd label, the agent plist's name and the bundle id are this one string.
    pub fn label(&self) -> String {
        self.bundle_id.clone().unwrap_or_else(|| "ai.jetto.cleat.rs.dev".into())
    }

    /// `.dev` and `.rs` builds never register a login agent: they share no bundle id with the
    /// release build, so registering would leave a second Cleat starting at login.
    pub fn is_development_build(&self) -> bool {
        is_development_label(&self.label())
    }

    pub fn version(&self) -> String {
        self.version.clone().unwrap_or_else(|| env!("CARGO_PKG_VERSION").into())
    }
}

pub fn is_development_label(label: &str) -> bool {
    label.ends_with(".dev") || label.ends_with(".rs")
}
