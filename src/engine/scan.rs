//! Parallel scanner: catalog sizing + conservative orphan detection.
//!
//! Orphan accuracy contract (per user requirement): a folder under
//! AppData/ProgramData is only proposed as a leftover when EVERY identity source
//! agrees it is unknown — installed-program registry, Appx packages, program
//! folders, Start Menu shortcuts, running processes, services and driver names —
//! and it has been idle for the configured number of days. Folders are never
//! bulk-deleted; each candidate is shown with evidence and selected individually.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::SystemTime;

use rayon::prelude::*;
use walkdir::WalkDir;

use super::paths::{self, Env};
use super::{Event, Kind, Orphan, ScanItem, ScanResult, Tier};
use crate::log;
use crate::platform;

// ------------------------------------------------------------- normalization

/// Lowercase alphanumeric fingerprint of a name.
pub fn norm(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_lowercase()
}

/// Sub-words of a name, normalized ("Epic Games" -> ["epic","games"]).
fn sub_words(s: &str) -> Vec<String> {
    s.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| w.len() >= 3)
        .map(norm)
        .filter(|w| !w.is_empty())
        .collect()
}

// --------------------------------------------------------------- size probing

#[cfg(windows)]
fn is_hidden(md: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    md.file_attributes() & 0x2 != 0
}

#[cfg(not(windows))]
fn is_hidden(md: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    md.permissions().mode() & 0o1777 == 0
}

/// (size, newest mtime) of a recursive tree.
pub fn tree_stats(p: &Path) -> (u64, Option<SystemTime>) {
    let mut size = 0u64;
    let mut newest: Option<SystemTime> = None;
    for entry in WalkDir::new(p)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if let Ok(md) = entry.metadata() {
            if md.is_file() {
                size += md.len();
                if let Ok(mt) = md.modified() {
                    if newest.map(|n| mt > n).unwrap_or(true) {
                        newest = Some(mt);
                    }
                }
            }
        }
    }
    (size, newest)
}

/// Size of files matching `patterns` directly inside `dir` (non-recursive).
fn pattern_size(dir: &Path, patterns: &[&str]) -> u64 {
    let mut size = 0u64;
    if let Ok(rd) = std::fs::read_dir(dir) {
        for entry in rd.flatten() {
            let name = entry.file_name().to_string_lossy().to_lowercase();
            if patterns.iter().any(|pat| matches_pattern(&name, pat)) {
                if let Ok(md) = entry.metadata() {
                    size += md.len();
                }
            }
        }
    }
    size
}

fn matches_pattern(name: &str, pattern: &str) -> bool {
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

// ------------------------------------------------------------ identity tokens

pub struct InstalledTokens {
    pub set: HashSet<String>,
}

fn collect_installed_tokens(env: &Env, tx: &Sender<Event>) -> InstalledTokens {
    let mut set: HashSet<String> = HashSet::new();

    let _ = tx.send(Event::ScanPhase(
        "Reading installed programs (registry)...".into(),
    ));

    const VALS: &[&str] = &["DisplayName", "Publisher", "InstallLocation"];
    for (path, hive) in [
        (
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
            platform::Hive::Hklm,
        ),
        (
            r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
            platform::Hive::Hklm,
        ),
        (
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
            platform::Hive::Hkcu,
        ),
    ] {
        for (_key, vals) in platform::reg_enum_with_values(hive, path, VALS) {
            // [DisplayName, Publisher, InstallLocation]
            for v in vals.iter().take(2).flatten() {
                if v.trim().is_empty() {
                    continue;
                }
                let n = norm(v);
                if n.len() >= 3 {
                    set.insert(n);
                }
                for w in sub_words(v) {
                    set.insert(w);
                }
            }
            if let Some(loc) = vals.get(2).and_then(|o| o.as_ref()) {
                if let Some(leaf) = std::path::Path::new(loc.trim_end_matches('\\')).file_name() {
                    let n = norm(&leaf.to_string_lossy());
                    if n.len() >= 3 {
                        set.insert(n);
                    }
                }
            }
        }
    }

    let _ = tx.send(Event::ScanPhase("Reading Store apps...".into()));
    if let Ok(out) =
        platform::powershell("Get-AppxPackage -AllUsers | Select-Object -ExpandProperty Name")
    {
        for line in out.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            set.insert(norm(line));
            for part in line.split(['.', '_']) {
                let n = norm(part);
                if n.len() >= 3 {
                    set.insert(n);
                }
            }
        }
    }

    let _ = tx.send(Event::ScanPhase(
        "Reading program folders & shortcuts...".into(),
    ));
    let mut program_roots: Vec<PathBuf> = vec![
        env.pf.clone(),
        env.pf86.clone(),
        env.local.join("Programs"),
        env.program_data.clone(),
    ];
    program_roots.sort();
    program_roots.dedup();
    for root in program_roots {
        if let Ok(rd) = std::fs::read_dir(&root) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                let n = norm(&name);
                if n.len() >= 3 {
                    set.insert(n);
                }
                for w in sub_words(&name) {
                    set.insert(w);
                }
            }
        }
    }

    for menu in [
        env.program_data
            .join(r"Microsoft\Windows\Start Menu\Programs"),
        env.roaming.join(r"Microsoft\Windows\Start Menu\Programs"),
    ] {
        for entry in WalkDir::new(&menu)
            .max_depth(6)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let name = entry.file_name().to_string_lossy();
            if name.ends_with(".lnk") {
                let stem = name.trim_end_matches(".lnk").to_string();
                let n = norm(&stem);
                if n.len() >= 3 {
                    set.insert(n);
                }
                for w in sub_words(&stem) {
                    set.insert(w);
                }
            }
        }
    }

    let _ = tx.send(Event::ScanPhase(
        "Reading services & running processes...".into(),
    ));
    for s in platform::service_names() {
        let n = norm(&s);
        if n.len() >= 3 {
            set.insert(n);
        }
    }
    for p in platform::running_process_names() {
        let n = norm(&p);
        if n.len() >= 3 {
            set.insert(n);
        }
    }

    InstalledTokens { set }
}

/// Names that belong to Windows, drivers or shared components — never candidates.
const PROTECTED_NAMES: &[&str] = &[
    "microsoft",
    "packages",
    "temp",
    "programs",
    "comms",
    "connecteddevices",
    "publishers",
    "placeholdertilelogofolder",
    "virtualstore",
    "d3dscache",
    "windows",
    "package cache",
    "usoprivate",
    "usoshared",
    "ssh",
    "nuget",
    "pip",
    "npm",
    "nvidia",
    "nvidia corporation",
    "intel",
    "amd",
    "apple",
    "apple computer",
    "history",
    "diagnostics",
    "crashdumps",
    "speech",
    "toolkit",
    "locallow",
    "oracle",
    "dell",
    "hp",
    "lenovo",
    "asus",
    "acer",
    "realtek",
    "microsoft onedrive",
    "onedrive",
    "google",
    "mozilla",
    "adobe",
    "capcut",
    "wdf",
    "regid.1991-06.com.microsoft",
    "soft",
    "desktop",
    "installer",
    "identitycrl",
    "common files",
    "windows apps",
    "internet explorer",
    "microsoft edge",
    "windows defender",
    "windows terminal",
    "modifiablewindowsapps",
    "servicing",
    "sxs",
    "winsxs",
    "prefetch",
    "rescache",
    "fonts",
    "shellcomponents",
    "cdr",
    "diagnostics performance",
    "games",
    "media",
    "online services",
    "packages2",
    "security",
    "settings",
    "shared libraries",
    "tempz",
];

// ---------------------------------------------------------------- orphan scan

fn looks_like_guid(name: &str) -> bool {
    let s = name.trim_matches(['{', '}']);
    s.len() >= 32 && s.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

fn orphan_candidates_for_root(
    _env: &Env,
    root: &Path,
    tokens: &InstalledTokens,
    age_days: u32,
    processes: &HashSet<String>,
    tx: &Sender<Event>,
) -> Vec<Orphan> {
    let mut out = Vec::new();
    let rd = match std::fs::read_dir(root) {
        Ok(r) => r,
        Err(_) => return out,
    };
    let now = SystemTime::now();
    let cutoff_secs = age_days as u64 * 86_400;

    for entry in rd.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let n = norm(&name);

        // --- identity filters ---------------------------------------------
        if name.starts_with('.') {
            continue;
        }
        if looks_like_guid(&name) {
            continue;
        }
        if PROTECTED_NAMES.contains(&name.to_lowercase().as_str()) {
            continue;
        }
        if n.len() < 3 {
            continue;
        }
        if let Ok(md) = entry.metadata() {
            if is_hidden(&md) {
                continue;
            }
        }

        // every sub-word of the folder name must be unknown
        let mut words = vec![n.clone()];
        words.extend(sub_words(&name));
        let mut matched_token: Option<String> = None;
        for w in &words {
            for t in &tokens.set {
                // either side containing the other means a likely identity match
                if t == w
                    || (w.len() >= 3 && t.contains(w.as_str()))
                    || (t.len() >= 4 && w.contains(t.as_str()))
                {
                    matched_token = Some(t.clone());
                    break;
                }
            }
            if matched_token.is_some() {
                break;
            }
        }
        if matched_token.is_some() {
            continue;
        }

        // running process with the same stem? keep.
        let pn = norm(&name);
        if processes.iter().any(|p| {
            let q = norm(p);
            q == pn || (pn.len() >= 4 && q.contains(&pn)) || (q.len() >= 4 && pn.contains(&q))
        }) {
            continue;
        }

        // --- age + size -----------------------------------------------------
        let (size, newest) = tree_stats(&path);
        let last = newest.unwrap_or_else(|| {
            entry
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .unwrap_or(now)
        });
        let idle_secs = now.duration_since(last).map(|d| d.as_secs()).unwrap_or(0);
        if idle_secs < cutoff_secs {
            continue;
        }

        let age_days_idle = idle_secs / 86_400;
        out.push(Orphan {
            path: path.clone(),
            name,
            size,
            last_used: Some(last),
            evidence: vec![
                format!(
                    "No match among {} installed-app identity tokens",
                    tokens.set.len()
                ),
                format!(
                    "Idle for {} days (last write {})",
                    age_days_idle,
                    chrono::DateTime::<chrono::Local>::from(last).format("%Y-%m-%d")
                ),
                "Not referenced by any running process or service".into(),
                format!("Root: {}", root.display()),
            ],
            selected: false,
        });
        let _ = tx.send(Event::ScanPhase(format!("Checking {} ...", path.display())));
    }
    out
}

// ------------------------------------------------------------------- packages

// ---------------------------------------------------------------- main driver

pub fn run_scan(
    env: Env,
    age_days: u32,
    tx: Sender<Event>,
    cancel: Arc<std::sync::atomic::AtomicBool>,
) {
    use std::sync::atomic::Ordering;
    let t_start = std::time::Instant::now();
    log::action("Scan started");

    let _ = tx.send(Event::ScanPhase("Collecting system identity...".into()));
    let tokens = collect_installed_tokens(&env, &tx);

    // ---------------- catalog sizing (parallel) ----------------
    let _ = tx.send(Event::ScanPhase("Sizing curated clean targets...".into()));
    let mut jobs: Vec<(usize, PathBuf)> = Vec::new();
    for (idx, def) in paths::TARGETS.iter().enumerate() {
        for p in (def.paths)(&env) {
            jobs.push((idx, p));
        }
    }
    let sized: Vec<(usize, PathBuf, u64, Option<SystemTime>)> = jobs
        .into_par_iter()
        .map(|(idx, p)| {
            if cancel.load(Ordering::Relaxed) {
                return (idx, p, 0, None);
            }
            let (size, newest) = match paths::TARGETS[idx].kind {
                Kind::Files => {
                    let s = pattern_size(&p, &patterns_for(paths::TARGETS[idx].id));
                    (s, newest_in_dir(&p))
                }
                _ => tree_stats(&p),
            };
            (idx, p, size, newest)
        })
        .collect();

    let mut items: Vec<ScanItem> = Vec::new();
    for (idx, def) in paths::TARGETS.iter().enumerate() {
        let own: Vec<_> = sized
            .iter()
            .filter(|(i, _, s, _)| *i == idx && *s > 0)
            .map(|(_, p, s, _)| (p.clone(), *s))
            .collect();
        if own.is_empty() {
            continue;
        }
        let newest = sized
            .iter()
            .filter(|(i, _, _, _)| *i == idx)
            .filter_map(|(_, _, _, n)| *n)
            .max();
        items.push(ScanItem {
            def_id: def.id,
            group: def.group,
            tier: def.tier,
            kind: def.kind,
            label: def.label.to_string(),
            desc: def.desc.to_string(),
            hint: def.hint.to_string(),
            special_confirm: def.special_confirm,
            size: own.iter().map(|(_, s)| s).sum(),
            paths: own,
            newest,
            selected: def.tier == Tier::Safe && !def.special_confirm,
        });
    }

    // ---------------- orphan scan ----------------
    let _ = tx.send(Event::ScanPhase(
        "Scanning AppData/ProgramData for leftovers...".into(),
    ));
    let processes = platform::running_process_names();
    let mut orphans = Vec::new();
    for root in [&env.local, &env.roaming, &env.program_data] {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        orphans.extend(orphan_candidates_for_root(
            &env, root, &tokens, age_days, &processes, &tx,
        ));
    }

    // ---------------- package managers ----------------
    let _ = tx.send(Event::ScanPhase("Detecting package managers...".into()));
    let packages = super::packages::detect(&env, &tx);

    // ---------------- browsers ----------------
    let _ = tx.send(Event::ScanPhase(
        "Checking browsers & update state...".into(),
    ));
    let browsers = super::browsers::detect(&env, &tx);

    let result = ScanResult {
        items,
        orphans: {
            orphans.sort_by_key(|o| std::cmp::Reverse(o.size));
            orphans
        },
        packages,
        browsers,
        token_count: tokens.set.len(),
        scanned_at: chrono::Local::now(),
    };

    log::ok(format!(
        "Scan finished in {:.1}s — {} catalog groups, {} orphan candidates, {} package managers, {} browsers",
        t_start.elapsed().as_secs_f64(),
        result.items.len(),
        result.orphans.len(),
        result.packages.iter().filter(|p| p.installed).count(),
        result.browsers.iter().filter(|b| b.installed).count(),
    ));
    let _ = tx.send(Event::ScanDone(Box::new(Ok(result))));
}

fn newest_in_dir(p: &Path) -> Option<SystemTime> {
    let mut newest: Option<SystemTime> = None;
    if let Ok(rd) = std::fs::read_dir(p) {
        for e in rd.flatten() {
            if let Ok(md) = e.metadata() {
                if md.is_file() {
                    if let Ok(mt) = md.modified() {
                        if newest.map(|n| mt > n).unwrap_or(true) {
                            newest = Some(mt);
                        }
                    }
                }
            }
        }
    }
    newest
}
