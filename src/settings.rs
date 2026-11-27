//! Persisted user settings (%APPDATA%/Purge/settings.json).

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

use crate::theme::Accent;

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Settings {
    pub accent: Accent,
    /// Create a system restore point before every clean (permanent mode safety net).
    pub restore_point: bool,
    /// Send deletions to the Recycle Bin instead of permanent delete.
    pub recycle_bin_mode: bool,
    /// Orphan candidate: folder must be idle at least this many days.
    pub orphan_age_days: u32,
    /// Show the confirmation modal before running a clean.
    pub confirm_before_clean: bool,
    /// Remembered original service start types for exact restore after "pause updates".
    pub saved_service_starts: HashMap<String, String>,
    /// Browsers whose update pause artifacts are currently applied (informational mirror).
    pub paused_browsers: Vec<String>,
    /// Total bytes freed across all sessions (lifetime stat).
    pub lifetime_freed: u64,
    /// Number of completed clean runs.
    pub clean_runs: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            accent: Accent::Violet,
            restore_point: true,
            recycle_bin_mode: false,
            orphan_age_days: 90,
            confirm_before_clean: true,
            saved_service_starts: HashMap::new(),
            paused_browsers: Vec::new(),
            lifetime_freed: 0,
            clean_runs: 0,
        }
    }
}

pub fn settings_path() -> PathBuf {
    let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("Purge").join("settings.json")
}

static GLOBAL: OnceLock<Mutex<Settings>> = OnceLock::new();

/// Install the global settings instance (call once at startup).
pub fn init(s: Settings) {
    let _ = GLOBAL.set(Mutex::new(s));
}

/// Snapshot of current settings.
pub fn get() -> Settings {
    GLOBAL
        .get()
        .and_then(|m| m.lock().ok().map(|s| s.clone()))
        .unwrap_or_default()
}

/// Mutate + persist the global settings.
pub fn update(f: impl FnOnce(&mut Settings)) {
    if let Some(m) = GLOBAL.get() {
        if let Ok(mut s) = m.lock() {
            f(&mut s);
            s.save();
        }
    }
}

impl Settings {
    pub fn load() -> Settings {
        let path = settings_path();
        fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let path = settings_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = fs::write(&path, json);
        }
    }
}
