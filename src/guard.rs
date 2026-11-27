//! The safety guard: every single deletion performed by Purge must pass through
//! `ensure_deletable`. It refuses to delete anything that is, or contains, or is a
//! parent of a personal folder (Documents, Desktop, Downloads, Pictures, Videos,
//! Music, OneDrive, user profile root) — with AppData/Temp carve-outs.

use std::path::{Path, PathBuf};

pub struct Guard {
    personal: Vec<PathBuf>,
    local: PathBuf,
    roaming: PathBuf,
    temp: PathBuf,
}

impl Guard {
    pub fn new() -> Guard {
        let personal: Vec<PathBuf> = [
            dirs::home_dir(),
            dirs::document_dir(),
            dirs::desktop_dir(),
            dirs::picture_dir(),
            dirs::video_dir(),
            dirs::audio_dir(),
            dirs::home_dir().map(|h| h.join("Downloads")),
            dirs::home_dir().map(|h| h.join("OneDrive")),
        ]
        .into_iter()
        .flatten()
        .map(|p| normalize(&p))
        .collect();

        let local = dirs::data_local_dir()
            .map(|p| normalize(&p))
            .unwrap_or_default();
        let roaming = dirs::config_dir()
            .map(|p| normalize(&p))
            .unwrap_or_default();
        let temp = std::env::var("TEMP")
            .ok()
            .map(|t| normalize(Path::new(&t)))
            .unwrap_or_default();

        Guard {
            personal,
            local,
            roaming,
            temp,
        }
    }

    fn under(&self, p: &Path, root: &Path) -> bool {
        if root.as_os_str().is_empty() {
            return false;
        }
        p.starts_with(root)
    }

    /// True when the path must NEVER be deleted.
    pub fn is_protected(&self, p: &Path) -> bool {
        let p = normalize(p);

        // Never delete roots of any kind.
        if p.parent().is_none() || p.as_os_str().is_empty() {
            return true;
        }
        if p.parent() == Some(p.ancestors().last().unwrap_or(Path::new(""))) {
            return true;
        }

        for pp in &self.personal {
            if pp.as_os_str().is_empty() {
                continue;
            }
            if p == *pp || p.starts_with(pp) || pp.starts_with(&p) {
                // carve-outs: AppData and Temp live under the profile but are not personal
                if self.under(&p, &self.local)
                    || self.under(&p, &self.roaming)
                    || self.under(&p, &self.temp)
                {
                    continue;
                }
                return true;
            }
        }
        false
    }

    /// Err(String) when the path must not be deleted. Ok(()) otherwise.
    pub fn ensure_deletable(&self, p: &Path) -> Result<(), String> {
        if self.is_protected(p) {
            return Err(format!(
                "protected path refused by safety guard: {}",
                p.display()
            ));
        }
        Ok(())
    }
}

fn normalize(p: &Path) -> PathBuf {
    let s = p
        .to_string_lossy()
        .trim_end_matches(['\\', '/'])
        .to_lowercase();
    PathBuf::from(s)
}

impl Default for Guard {
    fn default() -> Self {
        Self::new()
    }
}
