//! Cleaning engine: curated catalog + accurate orphan scanner + deletion core.

pub mod browsers;
pub mod clean;
pub mod packages;
pub mod paths;
pub mod scan;

use std::path::PathBuf;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

// ------------------------------------------------------------------ formatting

pub fn fmt_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.0} KB", b / KB)
    } else if b > 0.0 {
        format!("{bytes} B")
    } else {
        "0".into()
    }
}

// ------------------------------------------------------------------- taxonomy

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Group {
    SystemTemp,
    WindowsUpdate,
    CrashLogs,
    Caches,
    Browsers,
    Apps,
}

impl Group {
    pub fn title(&self) -> &'static str {
        match self {
            Group::SystemTemp => "System & temporary files",
            Group::WindowsUpdate => "Windows Update leftovers",
            Group::CrashLogs => "Crash dumps, error reports & logs",
            Group::Caches => "OS-level caches",
            Group::Browsers => "Browser caches",
            Group::Apps => "Application caches",
        }
    }

    pub fn subtitle(&self) -> &'static str {
        match self {
            Group::SystemTemp => "Temp folders, GPU shader caches, Prefetch",
            Group::WindowsUpdate => "Downloaded updates, Delivery Optimization",
            Group::CrashLogs => "Minidumps, WER reports, CBS & setup logs",
            Group::Caches => "Thumbnails, icon cache, font cache",
            Group::Browsers => "Cache only — passwords, history and bookmarks are never touched",
            Group::Apps => "Per-app cache folders — projects, drafts and saves stay intact",
        }
    }

    pub fn all() -> [Group; 6] {
        [
            Group::SystemTemp,
            Group::WindowsUpdate,
            Group::CrashLogs,
            Group::Caches,
            Group::Browsers,
            Group::Apps,
        ]
    }
}

/// Risk tier. `Safe` items start selected; `Moderate` start unselected.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tier {
    Safe,
    Moderate,
}

impl Tier {
    pub fn label(&self) -> &'static str {
        match self {
            Tier::Safe => "SAFE",
            Tier::Moderate => "MODERATE",
        }
    }
}

/// Deletion semantics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Delete the children of the folder, keep the folder itself.
    Contents,
    /// Delete the folder (and parents recreate what they need).
    WholeDir,
    /// Delete files matching a pattern directly inside the folder (non-recursive).
    Files,
}

pub type PathFn = fn(&paths::Env) -> Vec<PathBuf>;

#[derive(Clone, Copy)]
pub struct TargetDef {
    pub id: &'static str,
    pub group: Group,
    pub label: &'static str,
    pub desc: &'static str,
    pub tier: Tier,
    pub kind: Kind,
    pub paths: PathFn,
    /// Extra confirmation required (e.g. Windows.old rollback loss).
    pub special_confirm: bool,
    /// "Close X first" hint shown in the UI.
    pub hint: &'static str,
}

#[derive(Clone, Debug)]
pub struct ScanItem {
    pub def_id: &'static str,
    pub group: Group,
    pub tier: Tier,
    #[allow(dead_code)]
    pub kind: Kind,
    pub label: String,
    pub desc: String,
    pub hint: String,
    pub special_confirm: bool,
    /// Concrete existing paths with their sizes.
    #[allow(dead_code)]
    pub paths: Vec<(PathBuf, u64)>,
    pub size: u64,
    #[allow(dead_code)]
    pub newest: Option<SystemTime>,
    pub selected: bool,
}

/// Orphan candidate: top-level AppData/ProgramData folder that appears to belong
/// to an uninstalled application. Deletion is opt-in per item, with evidence.
#[derive(Clone, Debug)]
pub struct Orphan {
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
    pub last_used: Option<SystemTime>,
    pub evidence: Vec<String>,
    pub selected: bool,
}

#[derive(Clone, Debug)]
pub struct ScanResult {
    pub items: Vec<ScanItem>,
    pub orphans: Vec<Orphan>,
    pub packages: Vec<packages::PkgInfo>,
    pub browsers: Vec<browsers::BrowserInfo>,
    pub token_count: usize,
    pub scanned_at: chrono::DateTime<chrono::Local>,
}

impl ScanResult {
    pub fn selected_bytes(&self) -> u64 {
        self.items
            .iter()
            .filter(|i| i.selected)
            .map(|i| i.size)
            .sum::<u64>()
            + self
                .orphans
                .iter()
                .filter(|o| o.selected)
                .map(|o| o.size)
                .sum::<u64>()
    }
}

// ----------------------------------------------------------------- engine events

/// Progress payload streamed from worker threads to the UI.
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub enum Progress {
    Phase(String),
    Item(String),
    Cleaned { freed: u64, item: String },
    Failed { item: String, why: String },
}

#[derive(Clone, Debug)]
pub struct CleanSummary {
    pub freed: u64,
    pub items_deleted: u32,
    pub failures: Vec<(String, String)>,
    pub restore_point_ok: Option<bool>,
    #[allow(dead_code)]
    pub started_at: chrono::DateTime<chrono::Local>,
    pub duration_secs: f64,
}

#[allow(dead_code)]
pub enum Event {
    Log(crate::log::Level, String),
    ScanPhase(String),
    ScanDone(Box<Result<ScanResult, String>>),
    CleanProgress(Progress),
    CleanDone(Box<Result<CleanSummary, String>>),
}
