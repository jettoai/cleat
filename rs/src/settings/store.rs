//! The window's state (Swift `SettingsStore.swift`): one draft, loaded from the config file and
//! written back a moment after each edit. The file is the only truth; the daemon's watcher picks
//! the write up, so there is no IPC.

use std::path::PathBuf;

use crate::config::{paths, Config};
use crate::model::AudioDevice;

use super::document::{self, Origin, WriteError};
use super::draft::{LiveLevels, PairedDevice, SettingsDraft};
use super::sources;
use super::vitals::{read_process, DaemonVitals, Sampler, VitalsState};

#[derive(Debug, Clone, PartialEq)]
pub enum Phase {
    Loading,
    Ready,
    Unreadable(String),
}

pub struct Store {
    pub phase: Phase,
    pub draft: SettingsDraft,
    pub error_message: Option<String>,
    pub has_conflict: bool,
    pub present: Vec<AudioDevice>,
    pub live: LiveLevels,
    pub last_revert: Option<String>,
    /// status.json `stuck`, empty unless the daemon that wrote it is running.
    pub stuck: sources::Stuck,
    pub vitals: DaemonVitals,
    config_path: PathBuf,
    status_path: PathBuf,
    baseline: Option<Vec<u8>>,
    origin: Option<Origin>,
    pending: Option<(Config, Option<Vec<u8>>)>,
    sampler: Sampler,
}

impl Default for Store {
    fn default() -> Self {
        Self::new()
    }
}

impl Store {
    pub fn new() -> Self {
        Self {
            phase: Phase::Loading,
            draft: SettingsDraft::make(&Config::disabled(), &[], &[], &LiveLevels::default()),
            error_message: None,
            has_conflict: false,
            present: vec![],
            live: LiveLevels::default(),
            last_revert: None,
            stuck: sources::Stuck::default(),
            vitals: DaemonVitals::with_state(VitalsState::NotRunning),
            config_path: paths::config_path(),
            status_path: paths::status_path(),
            baseline: None,
            origin: None,
            pending: None,
            sampler: Sampler::default(),
        }
    }

    /// First half of a load: read and check the file, read the devices. The pairing list comes
    /// later through `apply_paired`. False when the file is unreadable.
    pub fn begin_load(&mut self) -> bool {
        self.phase = Phase::Loading;
        let loaded = match std::fs::read(&self.config_path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok((Config::disabled(), None)),
            Err(e) => Err(e.to_string()),
            Ok(data) => serde_json::from_slice::<Config>(&data)
                .map_err(|e| e.to_string())
                .and_then(|c| c.validate().map(|_| c).map_err(|e| e.to_string()))
                .map(|c| (c, Some(data))),
        };
        match loaded {
            Err(reason) => {
                self.phase = Phase::Unreadable(reason);
                false
            }
            Ok(p) => {
                self.pending = Some(p);
                self.present = sources::present_devices();
                self.refresh_live();
                true
            }
        }
    }

    pub fn apply_paired(&mut self, paired: &[PairedDevice]) {
        let Some((config, raw)) = self.pending.take() else { return };
        self.draft = SettingsDraft::make(&config, &self.present, paired, &self.live);
        self.origin = Some(Origin::new(raw.as_deref(), self.draft.managed()));
        self.baseline = raw;
        self.phase = Phase::Ready;
    }

    /// Device levels, and one read of status.json for the revert line, the stuck note and the vitals.
    pub fn refresh_live(&mut self) {
        self.live = sources::live_levels();
        let data = std::fs::read(&self.status_path).ok();
        self.last_revert = data.as_deref().and_then(sources::last_revert);
        let pid = data.as_deref().and_then(sources::status_pid);
        self.vitals = self.sampler.sample(pid, read_process);
        self.stuck = data.as_deref().map(sources::stuck).unwrap_or_default().shown_when(self.vitals.state);
        if self.vitals.state != VitalsState::NotRunning {
            self.vitals.performance = data.as_deref().and_then(sources::performance);
        }
    }

    pub fn transport_of(&self, name: &str) -> Option<u32> {
        self.present.iter().find(|d| d.name == name).map(|d| d.transport)
    }

    pub fn volume_candidates(&self) -> Vec<String> {
        self.draft.volume_candidates(&self.present)
    }

    /// Writes synchronously. A failure stays on screen.
    pub fn save_now(&mut self) {
        if self.phase != Phase::Ready {
            return;
        }
        match document::write(&self.config_path, &self.draft.managed(), self.origin.as_ref(), self.baseline.as_deref()) {
            Ok(bytes) => {
                self.baseline = Some(bytes);
                self.error_message = None;
            }
            Err(WriteError::ChangedOnDisk) => {
                self.has_conflict = true;
                self.error_message = Some(WriteError::ChangedOnDisk.to_string());
            }
            Err(e) => self.error_message = Some(e.to_string()),
        }
    }

    /// Discards the unsaved draft; the caller then loads again.
    pub fn reset_errors(&mut self) {
        self.error_message = None;
        self.has_conflict = false;
    }
}
