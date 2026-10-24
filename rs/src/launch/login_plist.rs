//! A plain launchd agent that runs `cleat login-start` at login. SMAppService's registration lives
//! in the Background Task Management database, which can lose it (B-2739: after a reboot nothing
//! started Cleat, and only Cleat itself re-registers). This file sits in `~/Library/LaunchAgents`,
//! outside that database: launchd loads it at login and `login-start` (cli/login_start.rs)
//! registers the SMAppService agent again and kicks it. It never uses `open`, which would pop the
//! settings window over a daemon that is already up.

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
    /// The running executable inside its `.app`; None when it sits in no bundle.
    pub exe: Option<PathBuf>,
}

impl LoginPlist {
    /// The real directory, and this executable if it sits in an `.app` (same lookup as `Identity`).
    pub fn current() -> Self {
        let exe = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.canonicalize().ok())
            .filter(|exe| crate::identity::enclosing_app(exe).is_some());
        Self { dir: crate::config::paths::home().join("Library/LaunchAgents"), exe }
    }

    fn path(&self) -> PathBuf {
        self.dir.join(format!("{LABEL}.plist"))
    }

    /// Makes the file match `wanted`; Some when it wrote or removed something. Nothing is changed
    /// when the content already matches, or when wanted but the executable's path is unknown.
    pub fn sync(&self, wanted: bool) -> Result<Option<Change>, String> {
        let path = self.path();
        if !wanted {
            return match std::fs::remove_file(&path) {
                Ok(()) => Ok(Some(Change::Removed)),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(e.to_string()),
            };
        }
        let Some(exe) = &self.exe else { return Ok(None) };
        let content = contents(exe);
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

/// Runs at load only (no KeepAlive, so a quit Cleat stays quit until the next login). The
/// executable path is one `<string>`, so it needs no quoting beyond XML escaping.
pub fn contents(exe: &Path) -> String {
    let exe = exe.to_string_lossy().replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>Label</key>
	<string>{LABEL}</string>
	<key>ProgramArguments</key>
	<array>
		<string>{exe}</string>
		<string>login-start</string>
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
