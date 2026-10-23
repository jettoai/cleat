//! The photos page's part of the draft: the folders and the long edge `heic` is converted to.

use std::path::Path;

use super::SettingsDraft;
use crate::config::HeicConfig;
use crate::photos::same_folder;

/// The long edges the page offers, in the order it shows them; 0 keeps the original size.
pub const LONG_EDGES: [u32; 5] = [1200, 1600, 2400, 3200, 0];

/// `~` for the home folder in a path as the config file would carry it.
fn tilde_in(home: &Path, path: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".into(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}

impl SettingsDraft {
    /// Kept as it was when the folder chosen is where the originals go now: one folder cannot be
    /// both.
    pub fn set_heic_watch(&mut self, chosen: &Path, home: &Path) {
        let originals = HeicConfig::originals_in(chosen, &self.heic_originals, home);
        if !same_folder(chosen, &originals) {
            self.heic_watch = tilde_in(home, chosen);
        }
    }

    /// A folder directly inside the watched one is kept as its name, so it follows the watch
    /// folder; any other is kept as a path. The watched folder itself is refused.
    pub fn set_heic_originals(&mut self, chosen: &Path, home: &Path) {
        let watch = crate::config::paths::expand_tilde(home, &self.heic_watch);
        if same_folder(chosen, &watch) {
            return;
        }
        let inside = chosen.parent() == Some(watch.as_path());
        self.heic_originals = match chosen.file_name() {
            Some(name) if inside => name.to_string_lossy().into_owned(),
            _ => tilde_in(home, chosen),
        };
    }
}
