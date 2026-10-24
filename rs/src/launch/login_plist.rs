//! A plain launchd agent that opens the app at login. SMAppService's registration lives in the
//! Background Task Management database, which can lose it (B-2739: after a reboot nothing started
//! Cleat, and only Cleat itself re-registers). This file sits in `~/Library/LaunchAgents`, outside
//! that database: launchd loads it at login, `open` starts the app, and the app's own sync then
//! registers the SMAppService agent again and hands over to it.

use std::path::{Path, PathBuf};

pub const LABEL: &str = "ai.jetto.cleat.login";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Written,
    Removed,
}

pub struct LoginPlist {
    /// `~/Library/LaunchAgents`; a tempdir in tests.
    pub dir: PathBuf,
    /// The running `.app`; None when the executable sits in no bundle.
    pub app: Option<PathBuf>,
}

impl LoginPlist {
    /// The real directory, and the `.app` this executable sits in (same lookup as `Identity`).
    pub fn current() -> Self {
        let app = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.canonicalize().ok())
            .and_then(|exe| crate::identity::enclosing_app(&exe).map(Path::to_path_buf));
        Self { dir: crate::config::paths::home().join("Library/LaunchAgents"), app }
    }

    fn path(&self) -> PathBuf {
        self.dir.join(format!("{LABEL}.plist"))
    }

    /// Makes the file match `wanted`; Some when it wrote or removed something. Nothing is changed
    /// when the content already matches, or when wanted but the app's path is unknown.
    pub fn sync(&self, wanted: bool) -> Result<Option<Change>, String> {
        let path = self.path();
        if !wanted {
            return match std::fs::remove_file(&path) {
                Ok(()) => Ok(Some(Change::Removed)),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(e.to_string()),
            };
        }
        let Some(app) = &self.app else { return Ok(None) };
        let content = contents(app);
        if std::fs::read_to_string(&path).is_ok_and(|have| have == content) {
            return Ok(None);
        }
        std::fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        // Rename, so launchd never reads a half-written file.
        let tmp = path.with_extension("plist.tmp");
        std::fs::write(&tmp, content).and_then(|_| std::fs::rename(&tmp, &path)).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            e.to_string()
        })?;
        Ok(Some(Change::Written))
    }
}

/// RunAtLoad only: no KeepAlive, so a quit Cleat stays quit until the next login.
pub fn contents(app: &Path) -> String {
    let app = app.to_string_lossy().replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>Label</key>
	<string>{LABEL}</string>
	<key>ProgramArguments</key>
	<array>
		<string>/usr/bin/open</string>
		<string>-g</string>
		<string>{app}</string>
	</array>
	<key>RunAtLoad</key>
	<true/>
	<key>ProcessType</key>
	<string>Interactive</string>
</dict>
</plist>
"#
    )
}
