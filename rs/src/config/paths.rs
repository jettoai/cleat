//! Every file cleat-rs reads or writes. Status and log live apart from the Swift build's so the
//! two never overwrite each other; the config file is shared.

use std::path::{Path, PathBuf};

pub const CONFIG_OVERRIDE_VARIABLE: &str = "CLEAT_CONFIG";

pub fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}

/// `{home}/.config/cleat/config.json`, unless the override names another file. Pure, for tests.
pub fn config_path_from(home: &Path, override_: Option<&str>) -> PathBuf {
    match override_ {
        Some(o) if !o.is_empty() => {
            if o == "~" {
                home.to_path_buf()
            } else if let Some(rest) = o.strip_prefix("~/") {
                home.join(rest)
            } else {
                PathBuf::from(o)
            }
        }
        _ => home.join(".config/cleat/config.json"),
    }
}

pub fn config_path() -> PathBuf {
    let var = std::env::var(CONFIG_OVERRIDE_VARIABLE).ok();
    config_path_from(&home(), var.as_deref())
}

pub fn support_dir() -> PathBuf {
    home().join("Library/Application Support/Cleat-rs")
}

pub fn status_path() -> PathBuf {
    support_dir().join("status.json")
}

pub fn log_dir() -> PathBuf {
    home().join("Library/Logs/Cleat-rs")
}

pub fn log_path() -> PathBuf {
    log_dir().join("cleat-rs.log")
}

pub fn rotated_log_path() -> PathBuf {
    log_dir().join("cleat-rs.log.1")
}

/// `~/...` for display.
pub fn tilde(path: &Path) -> String {
    let h = home();
    match path.strip_prefix(&h) {
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}
