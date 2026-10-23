//! Which files in the watched folder to convert and what to call the results. Pure: the folder's
//! listing goes in, jobs come out. No file system, no clock.

use std::collections::HashSet;
use std::time::SystemTime;

/// One item of the watched folder's first level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub size: u64,
    /// When the file appeared in this folder.
    pub born: SystemTime,
    /// Anything that is not a regular file: a folder, a link.
    pub is_dir: bool,
    /// The name was already there when the watch started. Only such a file can be "older": a name
    /// that was not there is new, however old its birth time (a copy may carry the source's).
    pub in_baseline: bool,
}

/// Why a HEIC-named item is left alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skip {
    /// Was in the folder before the watch started, and born before it: not touched.
    Older,
    /// A dot name: a download or sync tool's temporary file, or this module's own.
    Hidden,
    Folder,
}

impl Skip {
    pub fn reason(self) -> &'static str {
        match self {
            Skip::Older => "older than the watch",
            Skip::Hidden => "hidden or temporary name",
            Skip::Folder => "not a regular file",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    pub source: String,
    pub size: u64,
    /// `<stem>.jpg`, with ` 2`, ` 3` before the extension while the name is taken.
    pub output: String,
    /// The source's name inside the originals folder, numbered the same way.
    pub original: String,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Plan {
    pub jobs: Vec<Job>,
    pub skipped: Vec<(String, Skip)>,
}

fn split(name: &str) -> Option<(&str, &str)> {
    name.rsplit_once('.').filter(|(stem, _)| !stem.is_empty())
}

pub fn is_heic_name(name: &str) -> bool {
    split(name).is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("heic") || ext.eq_ignore_ascii_case("heif"))
}

/// The first of `stem.ext`, `stem 2.ext`, `stem 3.ext`... not in `taken` (compared without case,
/// like the volume), which it then joins.
fn unique(stem: &str, ext: &str, taken: &mut HashSet<String>) -> String {
    (1..)
        .map(|n| if n == 1 { format!("{stem}.{ext}") } else { format!("{stem} {n}.{ext}") })
        .find(|name| taken.insert(name.to_lowercase()))
        .expect("an unbounded range has a free name")
}

/// `existing` holds every name in the watched folder, `originals_existing` every name in the
/// originals folder. Jobs come oldest first.
pub fn plan(
    entries: &[Entry],
    started: SystemTime,
    existing: &HashSet<String>,
    originals_existing: &HashSet<String>,
) -> Plan {
    let mut taken: HashSet<String> = existing.iter().map(|n| n.to_lowercase()).collect();
    let mut originals: HashSet<String> = originals_existing.iter().map(|n| n.to_lowercase()).collect();
    let mut candidates: Vec<&Entry> = entries.iter().filter(|e| is_heic_name(&e.name)).collect();
    candidates.sort_by(|a, b| (a.born, &a.name).cmp(&(b.born, &b.name)));
    let mut plan = Plan::default();
    for e in candidates {
        let skip = if e.name.starts_with('.') {
            Some(Skip::Hidden)
        } else if e.is_dir {
            Some(Skip::Folder)
        } else if e.in_baseline && e.born < started {
            Some(Skip::Older)
        } else {
            None
        };
        if let Some(skip) = skip {
            plan.skipped.push((e.name.clone(), skip));
            continue;
        }
        let (stem, ext) = split(&e.name).expect("is_heic_name found an extension");
        plan.jobs.push(Job {
            source: e.name.clone(),
            size: e.size,
            output: unique(stem, "jpg", &mut taken),
            original: unique(stem, ext, &mut originals),
        });
    }
    plan
}
