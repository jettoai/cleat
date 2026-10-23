//! What the engine keeps of the HEIC watch for status.json `photos`: whether it runs, whether its
//! folder can be read, and what it has converted since the daemon started. The work is `photos`'.

use serde::{Deserialize, Serialize};

use super::Engine;
use crate::photos::Report;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PhotosState {
    Off,
    Watching,
    NoAccess,
}

impl PhotosState {
    pub fn label(self) -> &'static str {
        match self {
            PhotosState::Off => "off",
            PhotosState::Watching => "watching",
            PhotosState::NoAccess => "no-access",
        }
    }
}

/// The last picture converted: its original name and how long it took.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhotosLast {
    pub name: String,
    pub ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhotosStatus {
    pub state: PhotosState,
    pub converted: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last: Option<PhotosLast>,
}

#[derive(Debug, Default)]
pub(super) struct PhotosBook {
    converted: u64,
    last: Option<PhotosLast>,
    pub(super) no_access: bool,
}

impl Engine {
    pub(super) fn photos_status(&self) -> PhotosStatus {
        let state = match (&self.photos_applied, self.photos.no_access) {
            (None, _) => PhotosState::Off,
            (Some(_), true) => PhotosState::NoAccess,
            (Some(_), false) => PhotosState::Watching,
        };
        PhotosStatus { state, converted: self.photos.converted, last: self.photos.last.clone() }
    }

    /// Takes in one report; true when status.json now says something different.
    pub(super) fn photos_reported(&mut self, report: &Report) -> bool {
        match report {
            Report::Converted { from, ms, .. } => {
                self.photos.converted += 1;
                self.photos.last = Some(PhotosLast { name: from.clone(), ms: u64::try_from(*ms).unwrap_or(u64::MAX) });
                true
            }
            Report::NoAccess { .. } => !std::mem::replace(&mut self.photos.no_access, true),
            Report::Readable { .. } => std::mem::replace(&mut self.photos.no_access, false),
            Report::Skipped { .. } | Report::Failed { .. } => false,
        }
    }
}
