//! Package-manager registry: 25 managers — detection, cache cleaning,
//! full data removal and official-uninstaller lookup.

use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::sync::Arc;

use rayon::prelude::*;

use super::paths::Env;
use super::{fmt_size, Event, Kind};
use crate::guard::Guard;
use crate::log;
use crate::platform;
use crate::settings::Settings;

pub type CacheFn = fn(&Env) -> Vec<PathBuf>;

pub struct PkgDef {
    pub id: &'static str,
    pub name: &'static str,
    pub category: &'static str,
    /// Executable name used to detect the tool on PATH ("" = folder-presence only).
    pub exe: &'static str,
    pub cache_paths: CacheFn,
    pub full_paths: CacheFn,
    pub cache_kind: Kind,
    pub full_kind: Kind,
    /// Full removal also deletes the tool binary itself.
    pub removes_tool: bool,
    /// Registry DisplayName substrings for the official uninstaller lookup.
    pub uninstall_names: &'static [&'static str],
    pub note: &'static str,
}

#[derive(Clone, Debug)]
pub struct PkgInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub category: &'static str,
    pub note: &'static str,
    pub removes_tool: bool,
    pub cache_kind: Kind,
    pub full_kind: Kind,
    pub installed: bool,
    pub exe_path: Option<PathBuf>,
    pub cache_paths: Vec<(PathBuf, u64)>,
    pub full_paths: Vec<(PathBuf, u64)>,
    pub cache_size: u64,
    pub full_size: u64,
}

// ----------------------------------------------------------------- path helpers

fn existing_only(v: Vec<PathBuf>) -> Vec<PathBuf> {
    v.into_iter().filter(|p| p.exists()).collect()
}

// -------------------------------------------------------------- manager table

pub const MANAGERS: &[PkgDef] = &[
    PkgDef {
        id: "pip",
        name: "pip",
        category: "Python",
        exe: "pip",
        cache_paths: |e| vec![e.local.join(r"pip\cache")],
        full_paths: |e| vec![e.local.join(r"pip\cache")],
        cache_kind: Kind::Contents,
        full_kind: Kind::Contents,
        removes_tool: false,
        uninstall_names: &[],
        note: "pip ships inside Python — remove Python itself via its official uninstaller.",
    },
    PkgDef {
        id: "uv",
        name: "uv",
        category: "Python",
        exe: "uv",
        cache_paths: |e| vec![e.local.join(r"uv\cache")],
        full_paths: |e| vec![e.local.join("uv")],
        cache_kind: Kind::Contents,
        full_kind: Kind::WholeDir,
        removes_tool: true,
        uninstall_names: &[],
        note: "Full removal deletes the uv data dir; the launcher in %USERPROFILE%\\.local\\bin may remain.",
    },
    PkgDef {
        id: "poetry",
        name: "Poetry",
        category: "Python",
        exe: "poetry",
        cache_paths: |e| vec![e.local.join(r"pypoetry\Cache")],
        full_paths: |e| vec![e.local.join("pypoetry"), e.roaming.join("pypoetry")],
        cache_kind: Kind::Contents,
        full_kind: Kind::WholeDir,
        removes_tool: true,
        uninstall_names: &[],
        note: "WARNING: full removal also deletes Poetry-managed VIRTUAL ENVIRONMENTS (projects break).",
    },
    PkgDef {
        id: "pipx",
        name: "pipx",
        category: "Python",
        exe: "pipx",
        cache_paths: |e| vec![e.home.join(r".local\pipx\cache"), e.local.join(r"pipx\cache")],
        full_paths: |e| vec![e.home.join(r".local\pipx"), e.local.join("pipx")],
        cache_kind: Kind::Contents,
        full_kind: Kind::WholeDir,
        removes_tool: true,
        uninstall_names: &[],
        note: "WARNING: full removal deletes every pipx-installed CLI tool's venv.",
    },
    PkgDef {
        id: "conda",
        name: "Conda",
        category: "Python",
        exe: "conda",
        cache_paths: |e| vec![e.home.join(r".conda\pkgs")],
        full_paths: |e| vec![e.home.join(".conda")],
        cache_kind: Kind::Contents,
        full_kind: Kind::WholeDir,
        removes_tool: false,
        uninstall_names: &["anaconda3", "miniconda3", "miniforge3"],
        note: "Use 'official uninstaller' to remove the Anaconda/Miniconda distribution itself.",
    },
    PkgDef {
        id: "npm",
        name: "npm",
        category: "Node.js",
        exe: "npm",
        cache_paths: |e| vec![e.local.join("npm-cache"), e.roaming.join("npm-cache")],
        full_paths: |e| vec![e.roaming.join("npm"), e.local.join("npm-cache"), e.roaming.join("npm-cache")],
        cache_kind: Kind::Contents,
        full_kind: Kind::Contents,
        removes_tool: false,
        uninstall_names: &["node.js"],
        note: "Full removal deletes GLOBAL npm packages (%APPDATA%\\npm) — Node.js itself stays.",
    },
    PkgDef {
        id: "yarn",
        name: "Yarn",
        category: "Node.js",
        exe: "yarn",
        cache_paths: |e| vec![e.local.join(r"Yarn\Cache")],
        full_paths: |e| vec![e.local.join("Yarn"), e.roaming.join("Yarn")],
        cache_kind: Kind::Contents,
        full_kind: Kind::Contents,
        removes_tool: false,
        uninstall_names: &[],
        note: "",
    },
    PkgDef {
        id: "pnpm",
        name: "pnpm",
        category: "Node.js",
        exe: "pnpm",
        cache_paths: |e| vec![e.local.join(r"pnpm\store"), e.local.join("pnpm-cache"), e.local.join(r"pnpm\state")],
        full_paths: |e| vec![e.local.join("pnpm"), e.local.join("pnpm-cache")],
        cache_kind: Kind::Contents,
        full_kind: Kind::WholeDir,
        removes_tool: true,
        uninstall_names: &[],
        note: "Full removal deletes the content-addressable store and pnpm's own home (global bins).",
    },
    PkgDef {
        id: "bun",
        name: "Bun",
        category: "Node.js",
        exe: "bun",
        cache_paths: |e| vec![e.home.join(r".bun\install\cache")],
        full_paths: |e| vec![e.home.join(".bun")],
        cache_kind: Kind::Contents,
        full_kind: Kind::WholeDir,
        removes_tool: true,
        uninstall_names: &[],
        note: "bun.exe lives inside .bun — full removal uninstalls Bun itself.",
    },
    PkgDef {
        id: "deno",
        name: "Deno",
        category: "Node.js",
        exe: "deno",
        cache_paths: |e| vec![e.local.join(r"deno\deps"), e.local.join(r"deno\npm"), e.local.join(r"deno\registries"), e.local.join(r"deno\remote")],
        full_paths: |e| vec![e.local.join("deno"), e.home.join(".deno")],
        cache_kind: Kind::Contents,
        full_kind: Kind::WholeDir,
        removes_tool: true,
        uninstall_names: &[],
        note: "",
    },
    PkgDef {
        id: "cargo",
        name: "Cargo",
        category: "Rust",
        exe: "cargo",
        cache_paths: |e| vec![e.home.join(r".cargo\registry"), e.home.join(r".cargo\git")],
        full_paths: |e| vec![e.home.join(".cargo")],
        cache_kind: Kind::Contents,
        full_kind: Kind::WholeDir,
        removes_tool: true,
        uninstall_names: &[],
        note: "Full removal deletes .cargo (shims + registry). rustup toolchains remain under .rustup.",
    },
    PkgDef {
        id: "rustup",
        name: "rustup",
        category: "Rust",
        exe: "rustup",
        cache_paths: |e| vec![e.home.join(r"rustup\downloads"), e.home.join(r"rustup\tmp")],
        full_paths: |e| vec![e.home.join("rustup"), e.home.join(".cargo")],
        cache_kind: Kind::Contents,
        full_kind: Kind::WholeDir,
        removes_tool: true,
        uninstall_names: &[],
        note: "DANGER: full removal deletes EVERY Rust toolchain and Cargo. Rust projects stop compiling.",
    },
    PkgDef {
        id: "go",
        name: "Go",
        category: "Go",
        exe: "go",
        cache_paths: |e| vec![e.local.join("go-build"), e.home.join(r"go\pkg\mod\cache")],
        full_paths: |e| vec![e.home.join("go"), e.local.join("go-build")],
        cache_kind: Kind::Contents,
        full_kind: Kind::Contents,
        removes_tool: false,
        uninstall_names: &[],
        note: "Go toolchain in Program Files is untouched; modules re-download on next build.",
    },
    PkgDef {
        id: "nuget",
        name: "NuGet",
        category: ".NET",
        exe: "nuget",
        cache_paths: |e| vec![e.home.join(r".nuget\packages"), e.local.join(r"NuGet\http-cache"), e.local.join(r"NuGet\v3-cache"), e.local.join(r"NuGet\plugins-cache")],
        full_paths: |e| vec![e.home.join(".nuget"), e.local.join("NuGet")],
        cache_kind: Kind::Contents,
        full_kind: Kind::Contents,
        removes_tool: false,
        uninstall_names: &[],
        note: "`dotnet restore` / `nuget restore` re-downloads everything.",
    },
    PkgDef {
        id: "maven",
        name: "Maven",
        category: "JVM",
        exe: "mvn",
        cache_paths: |e| vec![e.home.join(r".m2\repository")],
        full_paths: |e| vec![e.home.join(".m2")],
        cache_kind: Kind::Contents,
        full_kind: Kind::Contents,
        removes_tool: false,
        uninstall_names: &[],
        note: "",
    },
    PkgDef {
        id: "gradle",
        name: "Gradle",
        category: "JVM",
        exe: "gradle",
        cache_paths: |e| vec![e.home.join(r".gradle\caches"), e.home.join(r".gradle\daemon")],
        full_paths: |e| vec![e.home.join(".gradle")],
        cache_kind: Kind::Contents,
        full_kind: Kind::Contents,
        removes_tool: false,
        uninstall_names: &[],
        note: "Wrapper distributions re-download on the next build.",
    },
    PkgDef {
        id: "sbt",
        name: "sbt / Ivy",
        category: "JVM",
        exe: "sbt",
        cache_paths: |e| vec![e.home.join(r".ivy2\cache"), e.home.join(r".sbt\boot")],
        full_paths: |e| vec![e.home.join(".ivy2"), e.home.join(".sbt")],
        cache_kind: Kind::Contents,
        full_kind: Kind::Contents,
        removes_tool: false,
        uninstall_names: &[],
        note: "",
    },
    PkgDef {
        id: "composer",
        name: "Composer",
        category: "PHP",
        exe: "composer",
        cache_paths: |e| vec![e.roaming.join(r"Composer\cache"), e.local.join(r"Composer\cache")],
        full_paths: |e| vec![e.roaming.join("Composer"), e.local.join("Composer")],
        cache_kind: Kind::Contents,
        full_kind: Kind::Contents,
        removes_tool: false,
        uninstall_names: &[],
        note: "composer.phar itself is not stored here.",
    },
    PkgDef {
        id: "choco",
        name: "Chocolatey",
        category: "System",
        exe: "choco",
        cache_paths: |e| vec![e.program_data.join(r"chocolatey\cache"), e.local.join("chocolatey")],
        full_paths: |e| vec![e.program_data.join("chocolatey")],
        cache_kind: Kind::Contents,
        full_kind: Kind::WholeDir,
        removes_tool: true,
        uninstall_names: &["chocolatey"],
        note: "DANGER: full removal deletes EVERY installed Chocolatey package.",
    },
    PkgDef {
        id: "scoop",
        name: "Scoop",
        category: "System",
        exe: "scoop",
        cache_paths: |e| vec![e.home.join(r"scoop\cache")],
        full_paths: |e| vec![e.home.join("scoop")],
        cache_kind: Kind::Contents,
        full_kind: Kind::WholeDir,
        removes_tool: true,
        uninstall_names: &[],
        note: "DANGER: full removal deletes every Scoop-installed app. PATH entries must be cleaned manually.",
    },
    PkgDef {
        id: "vcpkg",
        name: "vcpkg",
        category: "C/C++",
        exe: "vcpkg",
        cache_paths: |e| vec![e.local.join(r"vcpkg\archives")],
        full_paths: |e| vec![e.local.join("vcpkg")],
        cache_kind: Kind::Contents,
        full_kind: Kind::Contents,
        removes_tool: false,
        uninstall_names: &[],
        note: "Binary caches rebuild from source on next install.",
    },
    PkgDef {
        id: "cabal",
        name: "Cabal",
        category: "Haskell",
        exe: "cabal",
        cache_paths: |e| vec![e.roaming.join(r"cabal\packages"), e.home.join(r".cabal\packages")],
        full_paths: |e| vec![e.roaming.join("cabal"), e.home.join(".cabal")],
        cache_kind: Kind::Contents,
        full_kind: Kind::Contents,
        removes_tool: false,
        uninstall_names: &[],
        note: "",
    },
    PkgDef {
        id: "huggingface",
        name: "HuggingFace",
        category: "AI / ML",
        exe: "",
        cache_paths: |e| vec![e.home.join(r".cache\huggingface")],
        full_paths: |e| vec![e.home.join(r".cache\huggingface")],
        cache_kind: Kind::Contents,
        full_kind: Kind::Contents,
        removes_tool: false,
        uninstall_names: &[],
        note: "Model/dataset blobs re-download on demand — can be MANY GB.",
    },
    PkgDef {
        id: "torch",
        name: "PyTorch Hub",
        category: "AI / ML",
        exe: "",
        cache_paths: |e| vec![e.home.join(r".cache\torch")],
        full_paths: |e| vec![e.home.join(r".cache\torch")],
        cache_kind: Kind::Contents,
        full_kind: Kind::Contents,
        removes_tool: false,
        uninstall_names: &[],
        note: "Cached checkpoints re-download on demand.",
    },
    PkgDef {
        id: "playwright",
        name: "Playwright / Patchright",
        category: "AI / ML",
        exe: "",
        cache_paths: |e| vec![e.local.join("ms-playwright"), e.local.join("ms-playwright-patchright")],
        full_paths: |e| vec![e.local.join("ms-playwright"), e.local.join("ms-playwright-patchright")],
        cache_kind: Kind::Contents,
        full_kind: Kind::Contents,
        removes_tool: false,
        uninstall_names: &[],
        note: "Downloaded browser binaries — re-fetched with `playwright install` / `patchright install`.",
    },
];

// ------------------------------------------------------------------ detection

pub fn detect(env: &Env, tx: &Sender<Event>) -> Vec<PkgInfo> {
    let _ = tx.send(Event::ScanPhase("Checking package managers...".into()));
    let defs: Vec<&PkgDef> = MANAGERS.iter().collect();
    let infos: Vec<PkgInfo> = defs
        .par_iter()
        .map(|def| {
            let exe_path = if def.exe.is_empty() {
                None
            } else {
                platform::find_on_path(def.exe)
            };
            let cache_raw = existing_only((def.cache_paths)(env));
            let full_raw = existing_only((def.full_paths)(env));
            let cache_paths: Vec<(PathBuf, u64)> = cache_raw
                .iter()
                .map(|p| (p.clone(), super::scan::tree_stats(p).0))
                .collect();
            let full_paths: Vec<(PathBuf, u64)> = full_raw
                .iter()
                .filter(|p| !cache_paths.iter().any(|(cp, _)| p.starts_with(cp)))
                .map(|p| (p.clone(), super::scan::tree_stats(p).0))
                .collect();
            let cache_size = cache_paths.iter().map(|(_, s)| s).sum();
            let full_extra = full_paths.iter().map(|(_, s)| s).sum::<u64>();
            PkgInfo {
                id: def.id,
                name: def.name,
                category: def.category,
                note: def.note,
                removes_tool: def.removes_tool,
                cache_kind: def.cache_kind,
                full_kind: def.full_kind,
                installed: exe_path.is_some() || !cache_paths.is_empty() || !full_paths.is_empty(),
                exe_path,
                cache_paths,
                full_paths,
                cache_size,
                full_size: cache_size + full_extra,
            }
        })
        .collect();

    infos.into_iter().filter(|i| i.installed).collect()
}

pub fn find_info(env: &Env, id: &str) -> Option<PkgInfo> {
    let def = MANAGERS.iter().find(|d| d.id == id)?;
    let exe_path = if def.exe.is_empty() {
        None
    } else {
        platform::find_on_path(def.exe)
    };
    let cache_paths: Vec<(PathBuf, u64)> = existing_only((def.cache_paths)(env))
        .into_iter()
        .map(|p| {
            let s = super::scan::tree_stats(&p).0;
            (p, s)
        })
        .collect();
    let full_paths: Vec<(PathBuf, u64)> = existing_only((def.full_paths)(env))
        .into_iter()
        .filter(|p| !cache_paths.iter().any(|(cp, _)| p.starts_with(cp)))
        .map(|p| {
            let s = super::scan::tree_stats(&p).0;
            (p, s)
        })
        .collect();
    let cache_size = cache_paths.iter().map(|(_, s)| s).sum();
    let full_extra = full_paths.iter().map(|(_, s)| s).sum::<u64>();
    Some(PkgInfo {
        id: def.id,
        name: def.name,
        category: def.category,
        note: def.note,
        removes_tool: def.removes_tool,
        cache_kind: def.cache_kind,
        full_kind: def.full_kind,
        installed: true,
        exe_path,
        cache_paths,
        full_paths,
        cache_size,
        full_size: cache_size + full_extra,
    })
}

// ------------------------------------------------------------------- deletion

fn delete_paths(
    tx: &Sender<Event>,
    guard: &Guard,
    settings: &Settings,
    paths: &[(PathBuf, u64)],
    kind: Kind,
    label: &str,
    freed: &mut u64,
) {
    for (p, sz) in paths {
        if let Err(why) = guard.ensure_deletable(p) {
            log::error(format!("REFUSED {label}: {why}"));
            continue;
        }
        let _ = tx.send(Event::CleanProgress(super::Progress::Item(format!(
            "{label} — {}",
            p.display()
        ))));
        let result = match kind {
            Kind::Contents => {
                let mut local_freed = 0u64;
                if let Ok(rd) = std::fs::read_dir(p) {
                    for child in rd.flatten() {
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
                            Ok(()) => local_freed += s,
                            Err(e) => log::warn(format!("skip {}: {e}", cp.display())),
                        }
                    }
                }
                Ok(local_freed)
            }
            _ => {
                let r = if settings.recycle_bin_mode {
                    trash::delete(p).map_err(|e| format!("{e}"))
                } else if p.is_dir() {
                    std::fs::remove_dir_all(p).map_err(|e| format!("{e}"))
                } else {
                    std::fs::remove_file(p).map_err(|e| format!("{e}"))
                };
                r.map(|_| *sz)
            }
        };
        match result {
            Ok(f) => {
                *freed += f;
                log::ok(format!(
                    "{label}: {} freed from {}",
                    fmt_size(f),
                    p.display()
                ));
            }
            Err(e) => log::error(format!("{label}: {e}")),
        }
    }
}

pub fn clean_cache_of(
    tx: &Sender<Event>,
    env: &Env,
    settings: &Settings,
    guard: &Arc<Guard>,
    info: &PkgInfo,
    freed: &mut u64,
) {
    let _ = env;
    delete_paths(
        tx,
        guard,
        settings,
        &info.cache_paths,
        info.cache_kind,
        &format!("{} cache", info.name),
        freed,
    );
}

pub fn remove_all_of(
    tx: &Sender<Event>,
    env: &Env,
    settings: &Settings,
    guard: &Arc<Guard>,
    info: &PkgInfo,
    freed: &mut u64,
) {
    let _ = env;
    let all = info
        .full_paths
        .iter()
        .cloned()
        .chain(
            info.cache_paths
                .iter()
                .filter(|(cp, _)| !info.full_paths.iter().any(|(fp, _)| fp.starts_with(cp)))
                .cloned(),
        )
        .collect::<Vec<_>>();
    delete_paths(
        tx,
        guard,
        settings,
        &all,
        info.full_kind,
        &format!("{} full removal", info.name),
        freed,
    );
}

// --------------------------------------------------------- uninstaller lookup

/// Find the official uninstaller via Add/Remove Programs for managers like Conda.
pub fn find_official_uninstaller(hint: &[&str]) -> Option<(String, String)> {
    use crate::platform::Hive;
    if hint.is_empty() {
        return None;
    }
    for (path, hive) in [
        (
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
            Hive::Hklm,
        ),
        (
            r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
            Hive::Hklm,
        ),
        (
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
            Hive::Hkcu,
        ),
    ] {
        for (_subkey, vals) in
            platform::reg_enum_with_values(hive, path, &["DisplayName", "UninstallString"])
        {
            let dn = vals.first().cloned().flatten().unwrap_or_default();
            let us = vals.get(1).cloned().flatten().unwrap_or_default();
            if us.is_empty() {
                continue;
            }
            let dn_n = super::scan::norm(&dn);
            for h in hint {
                if dn_n.contains(&super::scan::norm(h)) {
                    return Some((dn, us));
                }
            }
        }
    }
    None
}

/// Launch an uninstall string (handles quoted exe + arguments), non-blocking.
pub fn launch_uninstaller(uninstall_string: &str) -> Result<(), String> {
    let s = uninstall_string.trim();
    let (exe, args): (String, String) = if let Some(rest) = s.strip_prefix('"') {
        let end = rest.find('"').map(|i| i + 1).unwrap_or(rest.len());
        (rest[..end].to_string(), rest[end..].trim().to_string())
    } else if let Some(idx) = s.to_lowercase().find(".exe") {
        let cut = idx + 4;
        (s[..cut].to_string(), s[cut..].trim().to_string())
    } else {
        (s.to_string(), String::new())
    };
    if exe.is_empty() {
        return Err("empty uninstall string".into());
    }
    std::process::Command::new(&exe)
        .args(args.split_whitespace())
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("launch {exe}: {e}"))
}
