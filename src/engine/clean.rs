//! Deletion core. Every delete flows through the Guard; permanent mode is the
//! default (user choice) with an optional Recycle-Bin mode in Settings; a system
//! restore point is created first when enabled.

use std::path::Path;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Instant;

use super::paths::{self, Env};
use super::{CleanSummary, Event, Kind, Progress};
use crate::guard::Guard;
use crate::log;
use crate::settings::Settings;

#[derive(Clone, Debug)]
pub struct CleanPlan {
    /// Selected catalog def ids (with the per-def explicit-confirm flag already accepted in UI).
    pub catalog: Vec<String>,
    /// Selected orphan paths.
    pub orphans: Vec<std::path::PathBuf>,
    /// Package manager ids: clean cache only.
    pub pkg_cache: Vec<String>,
    /// Package manager ids: full data removal.
    pub pkg_remove: Vec<String>,
    /// Browser ids to pause updates for.
    pub browser_pause: Vec<String>,
    /// Browser ids to clean cache for.
    pub browser_cache: Vec<String>,
}

fn matches_pattern(name: &str, pattern: &str) -> bool {
    let name = name.to_lowercase();
    if pattern == "*" {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix('*') {
        return name.starts_with(&prefix.to_lowercase());
    }
    if let Some(suffix) = pattern.strip_prefix('*') {
        return name.ends_with(&suffix.to_lowercase());
    }
    name == pattern.to_lowercase()
}

fn patterns_for(def_id: &str) -> Vec<&'static str> {
    let mut v = vec![paths::file_pattern_for(def_id)];
    v.extend_from_slice(paths::extra_file_patterns(def_id));
    v
}

fn tree_size(p: &Path) -> u64 {
    super::scan::tree_stats(p).0
}

fn remove_path(path: &Path, settings: &Settings) -> Result<(), String> {
    if settings.recycle_bin_mode {
        trash::delete(path).map_err(|e| format!("recycle bin: {e}"))
    } else if path.is_dir() {
        std::fs::remove_dir_all(path).map_err(|e| format!("{e}"))
    } else {
        std::fs::remove_file(path).map_err(|e| format!("{e}"))
    }
}

struct Cleaner {
    env: Env,
    settings: Settings,
    guard: Arc<Guard>,
    tx: Sender<Event>,
    cancel: Arc<std::sync::atomic::AtomicBool>,
    summary: CleanSummary,
    started: Instant,
}

impl Cleaner {
    fn cancelled(&self) -> bool {
        self.cancel.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn report(&self, p: Progress) {
        let _ = self.tx.send(Event::CleanProgress(p));
    }

    /// Delete one concrete path with `kind` semantics. Returns bytes freed.
    fn delete_one(&mut self, path: &Path, kind: Kind, patterns: &[&str], label: &str) -> u64 {
        if self.cancelled() {
            return 0;
        }
        if !path.exists() {
            return 0;
        }
        if let Err(why) = self.guard.ensure_deletable(path) {
            log::error(format!("REFUSED {label}: {why}"));
            self.summary
                .failures
                .push((format!("{label} (guard)"), why));
            return 0;
        }
        let before = match kind {
            Kind::Files => {
                let mut s = 0u64;
                if let Ok(rd) = std::fs::read_dir(path) {
                    for e in rd.flatten() {
                        let name = e.file_name().to_string_lossy().to_lowercase();
                        if patterns.iter().any(|p| matches_pattern(&name, p)) {
                            if let Ok(md) = e.metadata() {
                                s += md.len();
                            }
                        }
                    }
                }
                s
            }
            _ => tree_size(path),
        };
        if before == 0 {
            return 0;
        }
        self.report(Progress::Item(format!("{label} — {}", path.display())));

        let mut freed = 0u64;
        match kind {
            Kind::Contents => {
                if let Ok(rd) = std::fs::read_dir(path) {
                    for child in rd.flatten() {
                        if self.cancelled() {
                            break;
                        }
                        let cp = child.path();
                        let sz = tree_size(&cp);
                        if sz == 0 && cp.is_dir() {
                            // still try to remove empty dirs
                        }
                        match remove_path(&cp, &self.settings) {
                            Ok(()) => {
                                freed += sz;
                                self.summary.items_deleted += 1;
                            }
                            Err(e) => {
                                self.summary.failures.push((cp.display().to_string(), e));
                            }
                        }
                    }
                }
            }
            Kind::WholeDir => match remove_path(path, &self.settings) {
                Ok(()) => {
                    freed = before;
                    self.summary.items_deleted += 1;
                }
                Err(e) => {
                    self.summary.failures.push((path.display().to_string(), e));
                }
            },
            Kind::Files => {
                if let Ok(rd) = std::fs::read_dir(path) {
                    for child in rd.flatten() {
                        if self.cancelled() {
                            break;
                        }
                        let cp = child.path();
                        let name = child.file_name().to_string_lossy().to_lowercase();
                        if !patterns.iter().any(|p| matches_pattern(&name, p)) {
                            continue;
                        }
                        let sz = child.metadata().map(|m| m.len()).unwrap_or(0);
                        match remove_path(&cp, &self.settings) {
                            Ok(()) => {
                                freed += sz;
                                self.summary.items_deleted += 1;
                            }
                            Err(e) => {
                                self.summary.failures.push((cp.display().to_string(), e));
                            }
                        }
                    }
                }
            }
        }
        if freed > 0 {
            self.summary.freed += freed;
            self.report(Progress::Cleaned {
                freed,
                item: label.to_string(),
            });
        }
        freed
    }

    fn create_restore_point(&mut self) {
        if !self.settings.restore_point {
            self.summary.restore_point_ok = None;
            return;
        }
        self.report(Progress::Phase("Creating system restore point...".into()));
        log::action("Creating restore point (Checkpoint-Computer)...");
        let script = "try { Checkpoint-Computer -Description 'Purge cleaner' -RestorePointType MODIFY_SETTINGS -ErrorAction Stop; Write-Output OK } catch { Write-Output $_.Exception.Message }";
        match crate::platform::powershell(script) {
            Ok(out) if out.trim() == "OK" => {
                log::ok("Restore point created");
                self.summary.restore_point_ok = Some(true);
            }
            Ok(other) => {
                log::warn(format!(
                    "Restore point not created: {other}. (System Protection may be off, or one was made in the last 24h.)"
                ));
                self.summary.restore_point_ok = Some(false);
            }
            Err(e) => {
                log::warn(format!("Restore point failed: {e}"));
                self.summary.restore_point_ok = Some(false);
            }
        }
    }
}

/// Main clean job — runs on a worker thread.
pub fn run_clean(
    env: Env,
    plan: CleanPlan,
    settings: Settings,
    guard: Arc<Guard>,
    tx: Sender<Event>,
    cancel: Arc<std::sync::atomic::AtomicBool>,
) {
    let mut c = Cleaner {
        summary: CleanSummary {
            freed: 0,
            items_deleted: 0,
            failures: Vec::new(),
            restore_point_ok: None,
            started_at: chrono::Local::now(),
            duration_secs: 0.0,
        },
        env,
        settings,
        guard,
        tx,
        cancel,
        started: Instant::now(),
    };

    log::action("Clean started");
    c.report(Progress::Phase("Preparing...".into()));
    c.create_restore_point();

    // ---------------- catalog ----------------
    for def in paths::TARGETS
        .iter()
        .filter(|d| plan.catalog.iter().any(|id| id == d.id))
    {
        if c.cancelled() {
            break;
        }
        c.report(Progress::Phase(format!("Cleaning: {}", def.label)));
        if def.id == "wu_download" {
            for svc in ["wuauserv", "bits", "dosvc"] {
                if let Err(e) = crate::platform::service_stop(svc) {
                    log::warn(format!("stop {svc}: {e} (continuing)"));
                }
            }
            for p in (def.paths)(&c.env) {
                c.delete_one(&p, def.kind, &patterns_for(def.id), def.label);
            }
            for svc in ["bits", "wuauserv"] {
                let _ = crate::platform::service_start(svc);
            }
        } else {
            for p in (def.paths)(&c.env) {
                c.delete_one(&p, def.kind, &patterns_for(def.id), def.label);
            }
        }
    }

    // ---------------- orphans ----------------
    if !plan.orphans.is_empty() {
        c.report(Progress::Phase("Removing leftover app data...".into()));
        for p in &plan.orphans {
            if c.cancelled() {
                break;
            }
            let label = p
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| p.display().to_string());
            c.delete_one(p, Kind::WholeDir, &["*"], &format!("leftover: {label}"));
        }
    }

    // ---------------- package managers ----------------
    for id in &plan.pkg_cache {
        if c.cancelled() {
            break;
        }
        if let Some(info) = super::packages::find_info(&c.env, id) {
            c.report(Progress::Phase(format!("Package cache: {}", info.name)));
            super::packages::clean_cache_of(
                &c.tx,
                &c.env,
                &c.settings,
                &c.guard,
                &info,
                &mut c.summary.freed,
            );
        }
    }
    for id in &plan.pkg_remove {
        if c.cancelled() {
            break;
        }
        if let Some(info) = super::packages::find_info(&c.env, id) {
            c.report(Progress::Phase(format!("Package removal: {}", info.name)));
            super::packages::remove_all_of(
                &c.tx,
                &c.env,
                &c.settings,
                &c.guard,
                &info,
                &mut c.summary.freed,
            );
        }
    }

    // ---------------- browsers ----------------
    for id in &plan.browser_pause {
        if c.cancelled() {
            break;
        }
        c.report(Progress::Phase(format!("Pausing updates: {id}")));
        let msgs = super::browsers::pause_updates(id, &c.settings);
        for m in msgs {
            log::info(m);
        }
    }
    for id in &plan.browser_cache {
        if c.cancelled() {
            break;
        }
        c.report(Progress::Phase(format!("Browser cache: {id}")));
        let freed = super::browsers::clean_cache(id, &c.env, &c.settings, &c.guard);
        c.summary.freed += freed;
        log::ok(format!("Browser {id} cache cleaned"));
    }

    // ---------------- wrap up ----------------
    c.summary.duration_secs = c.started.elapsed().as_secs_f64();
    let freed = c.summary.freed;
    crate::settings::update(|s| {
        s.lifetime_freed += freed;
        s.clean_runs += 1;
    });
    log::ok(format!(
        "Clean finished — {} freed in {:.1}s ({} items, {} failures)",
        super::fmt_size(freed),
        c.summary.duration_secs,
        c.summary.items_deleted,
        c.summary.failures.len()
    ));
    let _ = c.tx.send(Event::CleanDone(Box::new(Ok(c.summary))));
}
