//! Browser engine: update pause/resume (scheduled tasks + services + official
//! policies) and cache cleaning. Cache clearing is strictly cache-only.

use std::path::PathBuf;
use std::sync::mpsc::Sender;

use super::paths::{self, Env};
use super::Event;
use crate::guard::Guard;
use crate::log;
use crate::platform::{self, Hive};
use crate::settings::Settings;

// ---------------------------------------------------------------- browser defs

pub struct BrowserDef {
    pub id: &'static str,
    pub name: &'static str,
    /// Chromium "User Data" root (cache discovered via profile subfolders).
    pub user_data: fn(&Env) -> Option<PathBuf>,
    /// Scheduled-task match: any-of substrings (lowercase).
    pub tasks_any: &'static [&'static str],
    /// Scheduled-task match: all-of substrings required (lowercase).
    pub tasks_all: &'static [&'static str],
    /// Exact service names (lowercase).
    pub services: &'static [&'static str],
    /// Registry policy path + values that disable updates.
    pub policy_path: &'static str,
    pub policy_values: &'static [(&'static str, u32)],
}

fn chromium_root(env: &Env, p: &str) -> Option<PathBuf> {
    let root = env.local.join(p);
    root.exists().then_some(root)
}

pub const BROWSERS: &[BrowserDef] = &[
    BrowserDef {
        id: "chrome",
        name: "Google Chrome",
        user_data: |e| chromium_root(e, r"Google\Chrome\User Data"),
        tasks_any: &["googleupdate"],
        tasks_all: &[],
        services: &["gupdate", "gupdatem"],
        policy_path: r"SOFTWARE\Policies\Google\Update",
        policy_values: &[("UpdateDefault", 0), ("AutoUpdateCheckPeriodMinutes", 0)],
    },
    BrowserDef {
        id: "edge",
        name: "Microsoft Edge",
        user_data: |e| chromium_root(e, r"Microsoft\Edge\User Data"),
        tasks_any: &["microsoftedgeupdate"],
        tasks_all: &[],
        services: &["edgeupdate", "edgeupdatem"],
        policy_path: r"SOFTWARE\Policies\Microsoft\EdgeUpdate",
        policy_values: &[("UpdateDefault", 0), ("AutoUpdateCheckPeriodMinutes", 0)],
    },
    BrowserDef {
        id: "brave",
        name: "Brave",
        user_data: |e| chromium_root(e, r"BraveSoftware\Brave-Browser\User Data"),
        tasks_any: &["braveupdate", "bravesoftwareupdate"],
        tasks_all: &[],
        services: &["brave"],
        policy_path: r"SOFTWARE\Policies\BraveSoftware\Update",
        policy_values: &[("UpdateDefault", 0), ("AutoUpdateCheckPeriodMinutes", 0)],
    },
    BrowserDef {
        id: "opera",
        name: "Opera",
        user_data: |e| chromium_root(e, "Opera Software\\Opera Stable"),
        tasks_any: &[],
        tasks_all: &["opera", "autoupdate"],
        services: &[],
        policy_path: "",
        policy_values: &[],
    },
    BrowserDef {
        id: "vivaldi",
        name: "Vivaldi",
        user_data: |e| chromium_root(e, r"Vivaldi\User Data"),
        tasks_any: &["vivaldi-update", "vivaldiupdate"],
        tasks_all: &[],
        services: &[],
        policy_path: r"SOFTWARE\Policies\Vivaldi\Update",
        policy_values: &[("UpdateDefault", 0), ("AutoUpdateCheckPeriodMinutes", 0)],
    },
    BrowserDef {
        id: "firefox",
        name: "Mozilla Firefox",
        user_data: |e| {
            let root = e.local.join("Mozilla").join("Firefox").join("Profiles");
            root.exists().then_some(root)
        },
        tasks_any: &[],
        tasks_all: &[],
        services: &["mozillamaintenance"],
        policy_path: r"SOFTWARE\Policies\Mozilla\Firefox",
        policy_values: &[("DisableAppUpdate", 1)],
    },
];

// ---------------------------------------------------------------- state model

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PauseState {
    Active,
    Partial,
    Paused,
}

impl PauseState {
    pub fn label(&self) -> &'static str {
        match self {
            PauseState::Active => "UPDATES ACTIVE",
            PauseState::Partial => "PARTIALLY PAUSED",
            PauseState::Paused => "UPDATES PAUSED",
        }
    }
}

#[derive(Clone, Debug)]
pub struct BrowserInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub installed: bool,
    pub cache_size: u64,
    pub cache_paths: Vec<(PathBuf, u64)>,
    pub pause: PauseState,
    pub task_hits: Vec<String>,
    pub service_hits: Vec<String>,
    #[allow(dead_code)]
    pub policy_present: bool,
}

fn task_matches(rule_any: &[&str], rule_all: &[&str], task_lower: &str) -> bool {
    if rule_any.iter().any(|s| task_lower.contains(s)) {
        return true;
    }
    !rule_all.is_empty() && rule_all.iter().all(|s| task_lower.contains(s))
}

fn collect_state(def: &BrowserDef) -> (Vec<String>, Vec<String>, bool) {
    let tasks = platform::schtasks_names()
        .into_iter()
        .filter(|t| {
            let lower = t.to_lowercase();
            task_matches(def.tasks_any, def.tasks_all, &lower)
        })
        .collect();
    let services: Vec<String> = def
        .services
        .iter()
        .filter(|s| platform::service_start_type(s).is_some())
        .map(|s| s.to_string())
        .collect();
    let policy_present = !def.policy_path.is_empty()
        && def
            .policy_values
            .iter()
            .any(|(v, _)| platform::reg_read_u32(Hive::Hklm, def.policy_path, v).is_some());
    (tasks, services, policy_present)
}

fn compute_pause(tasks: &[String], services: &[String], policy: bool) -> PauseState {
    let mut artifacts = 0usize;
    let mut blocked = 0usize;

    for t in tasks {
        artifacts += 1;
        if platform::schtasks_state(t)
            .map(|s| s.to_lowercase().contains("disabled"))
            .unwrap_or(false)
        {
            blocked += 1;
        }
    }
    for s in services {
        artifacts += 1;
        if platform::service_start_type(s).as_deref() == Some("disabled") {
            blocked += 1;
        }
    }
    if policy {
        artifacts += 1;
        blocked += 1;
    }

    match (artifacts, blocked) {
        (0, _) => PauseState::Active,
        (n, b) if b == n => PauseState::Paused,
        (_, b) if b > 0 => PauseState::Partial,
        _ => PauseState::Active,
    }
}

// ------------------------------------------------------------------ detection

pub fn detect(env: &Env, tx: &Sender<Event>) -> Vec<BrowserInfo> {
    let mut out = Vec::new();
    for def in BROWSERS {
        let _ = tx.send(Event::ScanPhase(format!("Checking {}...", def.name)));
        let cache_paths: Vec<(PathBuf, u64)> = cache_paths_for(env, def)
            .into_iter()
            .filter(|(p, _)| p.exists())
            .collect();
        let installed =
            !cache_paths.is_empty() || (def.user_data)(env).map(|r| r.exists()).unwrap_or(false);
        if !installed {
            continue;
        }
        let (tasks, services, policy) = collect_state(def);
        let pause = compute_pause(&tasks, &services, policy);
        out.push(BrowserInfo {
            id: def.id,
            name: def.name,
            installed,
            cache_size: cache_paths.iter().map(|(_, s)| s).sum(),
            cache_paths,
            pause,
            task_hits: tasks,
            service_hits: services,
            policy_present: policy,
        });
    }
    out
}

/// Concrete cache subfolders (Chromium profiles / Firefox cache2).
pub fn cache_paths_for(env: &Env, def: &BrowserDef) -> Vec<(PathBuf, u64)> {
    if def.id == "firefox" {
        let mut v = Vec::new();
        if let Some(root) = (def.user_data)(env) {
            if let Ok(rd) = std::fs::read_dir(&root) {
                for p in rd.flatten() {
                    let c2 = p.path().join("cache2");
                    if c2.exists() {
                        let s = super::scan::tree_stats(&c2).0;
                        v.push((c2, s));
                    }
                }
            }
        }
        return v;
    }
    if let Some(root) = (def.user_data)(env) {
        paths::chromium_cache_paths(env, root)
            .into_iter()
            .filter(|p| p.exists())
            .map(|p| {
                let s = super::scan::tree_stats(&p).0;
                (p, s)
            })
            .collect()
    } else {
        Vec::new()
    }
}

// ------------------------------------------------------------ pause / resume

fn def_by_id(id: &str) -> Option<&'static BrowserDef> {
    BROWSERS.iter().find(|d| d.id == id)
}

pub fn pause_updates(id: &str, settings: &Settings) -> Vec<String> {
    let mut msgs = Vec::new();
    let Some(def) = def_by_id(id) else {
        msgs.push(format!("Unknown browser id {id}"));
        return msgs;
    };
    log::action(format!("Pausing updates: {}", def.name));

    // scheduled tasks
    let tasks = platform::schtasks_names();
    let hits: Vec<String> = tasks
        .into_iter()
        .filter(|t| {
            let lower = t.to_lowercase();
            task_matches(def.tasks_any, def.tasks_all, &lower)
        })
        .collect();
    for t in hits {
        match platform::schtasks_set_enabled(&t, false) {
            Ok(()) => msgs.push(format!("Disabled scheduled task {t}")),
            Err(e) => msgs.push(format!("Task {t}: {e}")),
        }
    }

    // services (remember original start type for exact restore)
    for s in def.services {
        let orig = platform::service_start_type(s);
        if orig.as_deref() == Some("disabled") {
            msgs.push(format!("Service {s} already disabled"));
            continue;
        }
        if let Some(o) = orig.clone() {
            let mut st = settings.saved_service_starts.clone();
            st.insert(s.to_string(), o);
            crate::settings::update(|g| g.saved_service_starts = st.clone());
        }
        match platform::service_set_start(s, "disabled").and_then(|_| platform::service_stop(s)) {
            Ok(()) => msgs.push(format!("Service {s} disabled and stopped")),
            Err(e) => msgs.push(format!("Service {s}: {e}")),
        }
    }

    // registry policy
    if !def.policy_path.is_empty() {
        for (v, d) in def.policy_values {
            match platform::reg_write_u32(Hive::Hklm, def.policy_path, v, *d) {
                Ok(()) => msgs.push(format!("Policy set: {v} = {d}")),
                Err(e) => msgs.push(format!("Policy {v}: {e}")),
            }
        }
    }

    crate::settings::update(|g| {
        if !g.paused_browsers.iter().any(|b| b == id) {
            g.paused_browsers.push(id.to_string());
        }
    });
    msgs
}

pub fn resume_updates(id: &str) -> Vec<String> {
    let mut msgs = Vec::new();
    let Some(def) = def_by_id(id) else {
        msgs.push(format!("Unknown browser id {id}"));
        return msgs;
    };
    log::action(format!("Resuming updates: {}", def.name));

    let tasks = platform::schtasks_names();
    let hits: Vec<String> = tasks
        .into_iter()
        .filter(|t| {
            let lower = t.to_lowercase();
            task_matches(def.tasks_any, def.tasks_all, &lower)
        })
        .collect();
    for t in hits {
        match platform::schtasks_set_enabled(&t, true) {
            Ok(()) => msgs.push(format!("Enabled scheduled task {t}")),
            Err(e) => msgs.push(format!("Task {t}: {e}")),
        }
    }

    let saved = crate::settings::get().saved_service_starts;
    for s in def.services {
        let s = *s;
        let start = saved
            .get(s)
            .cloned()
            .unwrap_or_else(|| "demand".to_string());
        match platform::service_set_start(s, &start) {
            Ok(()) => msgs.push(format!("Service {s} restored to {start}")),
            Err(e) => msgs.push(format!("Service {s}: {e}")),
        }
    }

    if !def.policy_path.is_empty() {
        for (v, _) in def.policy_values {
            match platform::reg_delete_value(Hive::Hklm, def.policy_path, v) {
                Ok(()) => msgs.push(format!("Policy removed: {v}")),
                Err(e) => msgs.push(format!("Policy {v}: {e}")),
            }
        }
    }

    crate::settings::update(|g| {
        g.paused_browsers.retain(|b| b != id);
        g.saved_service_starts
            .retain(|k, _| !def.services.contains(&k.as_str()));
    });
    msgs
}

/// Live pause state for a browser id (used by the Browsers page).
pub fn live_state(id: &str) -> PauseState {
    let Some(def) = def_by_id(id) else {
        return PauseState::Active;
    };
    let (t, s, p) = collect_state(def);
    compute_pause(&t, &s, p)
}

// ----------------------------------------------------------------- cache clean

pub fn clean_cache(id: &str, env: &Env, settings: &Settings, guard: &Guard) -> u64 {
    let Some(def) = def_by_id(id) else { return 0 };
    let mut freed = 0u64;
    for (p, sz) in cache_paths_for(env, def) {
        if let Err(why) = guard.ensure_deletable(&p) {
            log::error(format!("REFUSED browser cache: {why}"));
            continue;
        }
        match delete_children(&p, settings) {
            Ok(f) => {
                freed += f;
                let _ = sz;
            }
            Err(e) => log::warn(format!("cache {}: {e}", p.display())),
        }
    }
    freed
}

fn delete_children(dir: &PathBuf, settings: &Settings) -> Result<u64, String> {
    let mut freed = 0u64;
    if !dir.exists() {
        return Ok(0);
    }
    for child in std::fs::read_dir(dir).map_err(|e| e.to_string())?.flatten() {
        let cp = child.path();
        let s = super::scan::tree_stats(&cp).0;
        let r = if settings.recycle_bin_mode {
            trash::delete(&cp).map_err(|e| format!("{e}"))
        } else if cp.is_dir() {
            std::fs::remove_dir_all(&cp).map_err(|e| format!("{e}"))
        } else {
            std::fs::remove_file(&cp).map_err(|e| format!("{e}"))
        };
        match r {
            Ok(()) => freed += s,
            Err(e) => log::warn(format!("skip {}: {e}", cp.display())),
        }
    }
    Ok(freed)
}
