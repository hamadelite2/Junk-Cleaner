//! Resolved environment paths + the curated clean-target catalog.
//!
//! Every entry is a well-known, safe-to-rebuild cache/temp location. The catalog
//! is whitelist-only: Purge never deletes anything that is not on this list, or
//! that the user explicitly selected from the orphan scan.

use std::path::PathBuf;

use super::{Group, Kind, TargetDef, Tier};

// ------------------------------------------------------------------- Env

#[derive(Clone, Debug)]
pub struct Env {
    pub local: PathBuf,
    pub roaming: PathBuf,
    pub program_data: PathBuf,
    pub windir: PathBuf,
    pub home: PathBuf,
    pub pf: PathBuf,
    pub pf86: PathBuf,
    pub temp: PathBuf,
    pub system_drive: PathBuf,
}

fn env_path(var: &str, fallback: &str) -> PathBuf {
    std::env::var_os(var)
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| PathBuf::from(fallback))
}

impl Env {
    pub fn detect() -> Env {
        let home = dirs::home_dir().unwrap_or_else(|| env_path("USERPROFILE", "C:\\Users\\Public"));
        let windir = env_path("WINDIR", "C:\\Windows");
        let pf = env_path("ProgramFiles", "C:\\Program Files");
        let pf86 = env_path("ProgramFiles(x86)", "C:\\Program Files (x86)");
        let local = dirs::data_local_dir().unwrap_or_else(|| home.join("AppData").join("Local"));
        let roaming = dirs::config_dir().unwrap_or_else(|| home.join("AppData").join("Roaming"));
        let temp = env_path("TEMP", &home.join("AppData\\Local\\Temp").to_string_lossy());
        let system_drive = {
            let s = windir.to_string_lossy();
            let cut = s.find('\\').map(|i| i.max(3)).unwrap_or(3).min(s.len());
            PathBuf::from(&s[..cut])
        };
        Env {
            local,
            roaming,
            program_data: env_path("ProgramData", "C:\\ProgramData"),
            system_drive,
            home,
            pf,
            pf86,
            temp,
            windir,
        }
    }
}

// ------------------------------------------------------- path-building helpers

fn one(p: PathBuf) -> Vec<PathBuf> {
    vec![p]
}

fn opt(p: PathBuf) -> Vec<PathBuf> {
    vec![p]
}

/// All Chromium profile cache subfolders for one browser root.
pub fn chromium_cache_paths(env: &Env, user_data: PathBuf) -> Vec<PathBuf> {
    const CACHE_SUBS: &[&str] = &[
        "Cache",
        "Code Cache",
        "GPUCache",
        "DawnGraphiteCache",
        "DawnWebGPUCache",
        r"Service Worker\CacheStorage",
        r"Service Worker\ScriptCache",
    ];
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&user_data) {
        for entry in rd.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let is_profile = name == "Default"
                || name == "Guest Profile"
                || (name.starts_with("Profile ") && name[8..].chars().all(|c| c.is_ascii_digit()));
            if is_profile && entry.path().is_dir() {
                for sub in CACHE_SUBS {
                    out.push(entry.path().join(sub));
                }
            }
        }
    }
    let _ = env;
    out
}

fn chromium_user_data(env: &Env, _vendor: &str, app: &str) -> PathBuf {
    env.local.join("Google").join(app).join("User Data")
}

fn chromium_crashpad(env: &Env, vendor: &str, app: &str) -> Vec<PathBuf> {
    let ud = chromium_user_data(env, vendor, app);
    opt(ud.join("Crashpad").join("completed"))
}

// ================================================================== CATALOG

pub const TARGETS: &[TargetDef] = &[
    // ---------------------------------------------------------- SystemTemp
    TargetDef {
        id: "user_temp",
        group: Group::SystemTemp,
        label: "User temp folder",
        desc: "Temporary files from all your applications (%TEMP%)",
        tier: Tier::Safe,
        kind: Kind::Contents,
        paths: |e| one(e.temp.clone()),
        special_confirm: false,
        hint: "",
    },
    TargetDef {
        id: "windows_temp",
        group: Group::SystemTemp,
        label: "Windows temp folder",
        desc: "System-level temporary files (%WINDIR%\\Temp)",
        tier: Tier::Safe,
        kind: Kind::Contents,
        paths: |e| one(e.windir.join("Temp")),
        special_confirm: false,
        hint: "",
    },
    TargetDef {
        id: "prefetch",
        group: Group::SystemTemp,
        label: "Prefetch traces",
        desc: "Launch-time traces rebuilt automatically as you use apps",
        tier: Tier::Safe,
        kind: Kind::Files,
        paths: |e| one(e.windir.join("Prefetch")),
        special_confirm: false,
        hint: "",
    },
    TargetDef {
        id: "directx_shader",
        group: Group::SystemTemp,
        label: "DirectX shader cache",
        desc: "Compiled shaders — rebuilt on first use by every game",
        tier: Tier::Safe,
        kind: Kind::Contents,
        paths: |e| one(e.local.join("D3DSCache")),
        special_confirm: false,
        hint: "",
    },
    TargetDef {
        id: "nvidia_dx_cache",
        group: Group::SystemTemp,
        label: "NVIDIA DirectX cache",
        desc: "NVIDIA GPU shader cache",
        tier: Tier::Safe,
        kind: Kind::Contents,
        paths: |e| one(e.local.join("NVIDIA").join("DXCache")),
        special_confirm: false,
        hint: "",
    },
    TargetDef {
        id: "nvidia_gl_cache",
        group: Group::SystemTemp,
        label: "NVIDIA OpenGL cache",
        desc: "NVIDIA OpenGL shader cache",
        tier: Tier::Safe,
        kind: Kind::Contents,
        paths: |e| one(e.local.join("NVIDIA").join("GLCache")),
        special_confirm: false,
        hint: "",
    },
    TargetDef {
        id: "nvidia_compute",
        group: Group::SystemTemp,
        label: "NVIDIA compute cache",
        desc: "NVIDIA CUDA compute cache",
        tier: Tier::Safe,
        kind: Kind::Contents,
        paths: |e| one(e.local.join("NVIDIA").join("ComputeCache")),
        special_confirm: false,
        hint: "",
    },
    TargetDef {
        id: "amd_shader",
        group: Group::SystemTemp,
        label: "AMD shader cache",
        desc: "AMD DirectX shader cache",
        tier: Tier::Safe,
        kind: Kind::Contents,
        paths: |e| one(e.local.join("AMD").join("DxCache")),
        special_confirm: false,
        hint: "",
    },
    TargetDef {
        id: "intel_shader",
        group: Group::SystemTemp,
        label: "Intel shader cache",
        desc: "Intel GPU shader cache",
        tier: Tier::Safe,
        kind: Kind::Contents,
        paths: |e| one(e.local.join("Intel").join("ShaderCache")),
        special_confirm: false,
        hint: "",
    },
    // ------------------------------------------------------ WindowsUpdate
    TargetDef {
        id: "wu_download",
        group: Group::WindowsUpdate,
        label: "Windows Update downloads",
        desc: "Already-installed update payloads (services stopped and restarted automatically)",
        tier: Tier::Safe,
        kind: Kind::Contents,
        paths: |e| one(e.windir.join("SoftwareDistribution").join("Download")),
        special_confirm: false,
        hint: "",
    },
    TargetDef {
        id: "delivery_opt",
        group: Group::WindowsUpdate,
        label: "Delivery Optimization cache",
        desc: "P2P-uploaded update chunks shared with other PCs",
        tier: Tier::Safe,
        kind: Kind::Contents,
        paths: |e| {
            one(
            e.windir
                .join(r"ServiceProfiles\NetworkService\AppData\Local\Microsoft\Windows\DeliveryOptimization\Cache"),
        )
        },
        special_confirm: false,
        hint: "",
    },
    TargetDef {
        id: "windows_old",
        group: Group::WindowsUpdate,
        label: "Windows.old / upgrade leftovers",
        desc: "Previous Windows install. DELETING REMOVES THE ABILITY TO ROLL BACK.",
        tier: Tier::Moderate,
        kind: Kind::WholeDir,
        paths: |e| {
            vec![
                e.system_drive.join("Windows.old"),
                e.system_drive.join("$WINDOWS.~BT"),
                e.system_drive.join("$WINDOWS.~WS"),
            ]
        },
        special_confirm: true,
        hint: "You will no longer be able to roll back to the previous Windows version",
    },
    // ---------------------------------------------------------- CrashLogs
    TargetDef {
        id: "minidumps",
        group: Group::CrashLogs,
        label: "Kernel minidumps",
        desc: "Blue-screen diagnostic dumps (%WINDIR%\\Minidump)",
        tier: Tier::Safe,
        kind: Kind::Contents,
        paths: |e| one(e.windir.join("Minidump")),
        special_confirm: false,
        hint: "",
    },
    TargetDef {
        id: "kernel_reports",
        group: Group::CrashLogs,
        label: "Live kernel reports",
        desc: "Live kernel hang reports",
        tier: Tier::Safe,
        kind: Kind::Contents,
        paths: |e| one(e.windir.join("LiveKernelReports")),
        special_confirm: false,
        hint: "",
    },
    TargetDef {
        id: "memory_dmp",
        group: Group::CrashLogs,
        label: "Full memory dump",
        desc: "Complete RAM snapshot written at crash time (can be several GB)",
        tier: Tier::Safe,
        kind: Kind::Files,
        paths: |e| one(e.windir.clone()),
        special_confirm: false,
        hint: "",
    },
    TargetDef {
        id: "wer_archive",
        group: Group::CrashLogs,
        label: "Error reports (archive)",
        desc: "Windows Error Reporting archived reports",
        tier: Tier::Safe,
        kind: Kind::Contents,
        paths: |e| one(e.program_data.join(r"Microsoft\Windows\WER\ReportArchive")),
        special_confirm: false,
        hint: "",
    },
    TargetDef {
        id: "wer_queue",
        group: Group::CrashLogs,
        label: "Error reports (queue)",
        desc: "Windows Error Reporting queued reports",
        tier: Tier::Safe,
        kind: Kind::Contents,
        paths: |e| one(e.program_data.join(r"Microsoft\Windows\WER\ReportQueue")),
        special_confirm: false,
        hint: "",
    },
    TargetDef {
        id: "wer_temp",
        group: Group::CrashLogs,
        label: "Error reports (temp)",
        desc: "Windows Error Reporting temp files",
        tier: Tier::Safe,
        kind: Kind::Contents,
        paths: |e| one(e.program_data.join(r"Microsoft\Windows\WER\Temp")),
        special_confirm: false,
        hint: "",
    },
    TargetDef {
        id: "wer_user",
        group: Group::CrashLogs,
        label: "User error reports",
        desc: "Per-user Windows Error Reporting data",
        tier: Tier::Safe,
        kind: Kind::Contents,
        paths: |e| one(e.local.join(r"Microsoft\Windows\WER")),
        special_confirm: false,
        hint: "",
    },
    TargetDef {
        id: "crash_dumps",
        group: Group::CrashLogs,
        label: "App crash dumps",
        desc: "Per-user application crash dumps (%LOCALAPPDATA%\\CrashDumps)",
        tier: Tier::Safe,
        kind: Kind::Contents,
        paths: |e| one(e.local.join("CrashDumps")),
        special_confirm: false,
        hint: "",
    },
    TargetDef {
        id: "panther_logs",
        group: Group::CrashLogs,
        label: "Windows setup logs",
        desc: "Windows upgrade/setup logs (Panther)",
        tier: Tier::Moderate,
        kind: Kind::Files,
        paths: |e| one(e.windir.join("Panther")),
        special_confirm: false,
        hint: "",
    },
    TargetDef {
        id: "cbs_logs",
        group: Group::CrashLogs,
        label: "Component store logs (CBS)",
        desc: "Servicing logs — safe to clear, repopulated as needed",
        tier: Tier::Safe,
        kind: Kind::Files,
        paths: |e| one(e.windir.join(r"Logs\CBS")),
        special_confirm: false,
        hint: "",
    },
    // ------------------------------------------------------------- Caches
    TargetDef {
        id: "thumbcache",
        group: Group::Caches,
        label: "Thumbnail cache",
        desc: "Explorer thumbnail images, rebuilt as you browse folders",
        tier: Tier::Safe,
        kind: Kind::Files,
        paths: |e| one(e.local.join(r"Microsoft\Windows\Explorer")),
        special_confirm: false,
        hint: "Locked thumbnails are skipped automatically",
    },
    TargetDef {
        id: "font_cache",
        group: Group::Caches,
        label: "Font cache",
        desc: "System font cache (rebuilds automatically)",
        tier: Tier::Moderate,
        kind: Kind::Contents,
        paths: |e| {
            one(e
                .windir
                .join(r"ServiceProfiles\LocalService\AppData\Local\FontCache"))
        },
        special_confirm: false,
        hint: "",
    },
    TargetDef {
        id: "directx_migration",
        group: Group::Caches,
        label: "Windows orphaned feature files",
        desc: "Windows orphaned components metadata (rarely large)",
        tier: Tier::Safe,
        kind: Kind::Contents,
        paths: |e| one(e.windir.join("System32").join("LogFiles").join("Scm")),
        special_confirm: false,
        hint: "",
    },
    // ----------------------------------------------------------- Browsers
    TargetDef {
        id: "chrome_cache",
        group: Group::Browsers,
        label: "Chrome cache",
        desc: "Web cache only — passwords, history, bookmarks untouched",
        tier: Tier::Safe,
        kind: Kind::Contents,
        paths: |e| chromium_cache_paths(e, chromium_user_data(e, "Google", "Chrome")),
        special_confirm: false,
        hint: "Close Chrome first",
    },
    TargetDef {
        id: "chrome_crash",
        group: Group::Browsers,
        label: "Chrome crash reports",
        desc: "Chromium Crashpad completed reports",
        tier: Tier::Safe,
        kind: Kind::Contents,
        paths: |e| chromium_crashpad(e, "Google", "Chrome"),
        special_confirm: false,
        hint: "",
    },
    TargetDef {
        id: "edge_cache",
        group: Group::Browsers,
        label: "Edge cache",
        desc: "Web cache only — passwords, history, bookmarks untouched",
        tier: Tier::Safe,
        kind: Kind::Contents,
        paths: |e| chromium_cache_paths(e, chromium_user_data(e, "Microsoft", "Edge")),
        special_confirm: false,
        hint: "Close Edge first",
    },
    TargetDef {
        id: "edge_crash",
        group: Group::Browsers,
        label: "Edge crash reports",
        desc: "Chromium Crashpad completed reports",
        tier: Tier::Safe,
        kind: Kind::Contents,
        paths: |e| chromium_crashpad(e, "Microsoft", "Edge"),
        special_confirm: false,
        hint: "",
    },
    TargetDef {
        id: "brave_cache",
        group: Group::Browsers,
        label: "Brave cache",
        desc: "Web cache only — passwords, history, bookmarks untouched",
        tier: Tier::Safe,
        kind: Kind::Contents,
        paths: |e| {
            chromium_cache_paths(
                e,
                e.local
                    .join("BraveSoftware")
                    .join("Brave-Browser")
                    .join("User Data"),
            )
        },
        special_confirm: false,
        hint: "Close Brave first",
    },
    TargetDef {
        id: "opera_cache",
        group: Group::Browsers,
        label: "Opera cache",
        desc: "Web cache only — passwords, history, bookmarks untouched",
        tier: Tier::Safe,
        kind: Kind::Contents,
        paths: |e| chromium_cache_paths(e, e.local.join("Opera Software").join("Opera Stable")),
        special_confirm: false,
        hint: "Close Opera first",
    },
    TargetDef {
        id: "vivaldi_cache",
        group: Group::Browsers,
        label: "Vivaldi cache",
        desc: "Web cache only — passwords, history, bookmarks untouched",
        tier: Tier::Safe,
        kind: Kind::Contents,
        paths: |e| chromium_cache_paths(e, e.local.join("Vivaldi").join("User Data")),
        special_confirm: false,
        hint: "Close Vivaldi first",
    },
    TargetDef {
        id: "firefox_cache",
        group: Group::Browsers,
        label: "Firefox cache",
        desc: "cache2 entries only — logins, history and bookmarks untouched",
        tier: Tier::Safe,
        kind: Kind::Contents,
        paths: |e| {
            let mut out = Vec::new();
            let root = e.local.join("Mozilla").join("Firefox").join("Profiles");
            if let Ok(rd) = std::fs::read_dir(&root) {
                for p in rd.flatten() {
                    out.push(p.path().join("cache2"));
                }
            }
            out
        },
        special_confirm: false,
        hint: "Close Firefox first",
    },
    TargetDef {
        id: "thunderbird_cache",
        group: Group::Browsers,
        label: "Thunderbird cache",
        desc: "Mail client cache — messages stay intact",
        tier: Tier::Safe,
        kind: Kind::Contents,
        paths: |e| {
            let mut out = Vec::new();
            let root = e.local.join("Thunderbird").join("Profiles");
            if let Ok(rd) = std::fs::read_dir(&root) {
                for p in rd.flatten() {
                    out.push(p.path().join("cache2"));
                }
            }
            out
        },
        special_confirm: false,
        hint: "Close Thunderbird first",
    },
    // --------------------------------------------------------------- Apps
    TargetDef {
        id: "discord_cache",
        group: Group::Apps,
        label: "Discord cache",
        desc: "Image/media cache — login and servers untouched",
        tier: Tier::Safe,
        kind: Kind::Contents,
        paths: |e| {
            let mut out = Vec::new();
            for app in ["discord", "discordcanary", "discordptb"] {
                for sub in ["Cache", "Code Cache", "GPUCache"] {
                    out.push(e.roaming.join(app).join(sub));
                }
            }
            out
        },
        special_confirm: false,
        hint: "Close Discord first",
    },
    TargetDef {
        id: "teams_classic",
        group: Group::Apps,
        label: "Teams (classic) cache",
        desc: "Cache, blob storage and GPU cache — chats untouched",
        tier: Tier::Safe,
        kind: Kind::Contents,
        paths: |e| {
            ["Cache", "tmp", "blob_storage", "GPUCache", "Code Cache"]
                .iter()
                .map(|s| e.roaming.join("Microsoft").join("Teams").join(s))
                .collect()
        },
        special_confirm: false,
        hint: "Close Teams first",
    },
    TargetDef {
        id: "teams_new_logs",
        group: Group::Apps,
        label: "Teams (new) logs",
        desc: "Diagnostic logs of the new Teams client",
        tier: Tier::Safe,
        kind: Kind::Contents,
        paths: |e| {
            let base = e
                .local
                .join("Packages")
                .join("MSTeams_8wekyb3d8bbwe")
                .join(r"LocalCache\Microsoft\MSTeams");
            ["Logs", "GBLogs"].iter().map(|s| base.join(s)).collect()
        },
        special_confirm: false,
        hint: "",
    },
    TargetDef {
        id: "slack_cache",
        group: Group::Apps,
        label: "Slack cache",
        desc: "Web cache — workspaces and login untouched",
        tier: Tier::Safe,
        kind: Kind::Contents,
        paths: |e| {
            [
                "Cache",
                "Code Cache",
                "GPUCache",
                r"Service Worker\CacheStorage",
            ]
            .iter()
            .map(|s| e.roaming.join("Slack").join(s))
            .collect()
        },
        special_confirm: false,
        hint: "Close Slack first",
    },
    TargetDef {
        id: "spotify_cache",
        group: Group::Apps,
        label: "Spotify storage",
        desc: "Streamed music cache — OFFLINE DOWNLOADS ARE REMOVED",
        tier: Tier::Moderate,
        kind: Kind::Contents,
        paths: |e| {
            vec![
                e.local.join("Spotify").join("Storage"),
                e.local.join("Spotify").join("Data"),
            ]
        },
        special_confirm: false,
        hint: "Close Spotify first",
    },
    TargetDef {
        id: "adobe_cache",
        group: Group::Apps,
        label: "Adobe media cache",
        desc: "Premiere/After Effects conform + peak files — re-rendered on next open",
        tier: Tier::Moderate,
        kind: Kind::Contents,
        paths: |e| {
            vec![
                e.roaming.join(r"Adobe\Common\Media Cache Files"),
                e.roaming.join(r"Adobe\Common\Media Cache"),
                e.roaming.join(r"Adobe\Common\Peak Files"),
            ]
        },
        special_confirm: false,
        hint: "Close Adobe apps first",
    },
    TargetDef {
        id: "steam_htmlcache",
        group: Group::Apps,
        label: "Steam web cache",
        desc: "Steam client store pages cache — games untouched",
        tier: Tier::Safe,
        kind: Kind::Contents,
        paths: |e| one(e.pf86.join("Steam").join("htmlcache")),
        special_confirm: false,
        hint: "Close Steam first",
    },
    TargetDef {
        id: "capcut_cache",
        group: Group::Apps,
        label: "CapCut cache",
        desc: "GPU/web caches only — DRAFTS AND PROJECTS ARE NEVER TOUCHED",
        tier: Tier::Safe,
        kind: Kind::Contents,
        paths: |e| {
            ["Cache", "GPUCache", "Code Cache"]
                .iter()
                .map(|s| e.local.join("CapCut").join("User Data").join(s))
                .collect()
        },
        special_confirm: false,
        hint: "Close CapCut first",
    },
    TargetDef {
        id: "vscode_cache",
        group: Group::Apps,
        label: "VS Code cache & logs",
        desc: "Editor cache/logs — settings, extensions and projects untouched",
        tier: Tier::Safe,
        kind: Kind::Contents,
        paths: |e| {
            ["Cache", "CachedData", "Code Cache", "GPUCache", "logs"]
                .iter()
                .map(|s| e.roaming.join("Code").join(s))
                .collect()
        },
        special_confirm: false,
        hint: "Close VS Code first",
    },
    TargetDef {
        id: "obs_logs",
        group: Group::Apps,
        label: "OBS logs",
        desc: "Recording/broadcast session logs — scenes and profiles untouched",
        tier: Tier::Safe,
        kind: Kind::Contents,
        paths: |e| one(e.roaming.join("obs-studio").join("logs")),
        special_confirm: false,
        hint: "Close OBS first",
    },
    TargetDef {
        id: "zoom_logs",
        group: Group::Apps,
        label: "Zoom logs",
        desc: "Meeting diagnostic logs",
        tier: Tier::Safe,
        kind: Kind::Contents,
        paths: |e| one(e.roaming.join("Zoom").join("logs")),
        special_confirm: false,
        hint: "Close Zoom first",
    },
    TargetDef {
        id: "onedrive_logs",
        group: Group::Apps,
        label: "OneDrive logs",
        desc: "Sync diagnostic logs — your files untouched",
        tier: Tier::Safe,
        kind: Kind::Contents,
        paths: |e| one(e.local.join("Microsoft").join("OneDrive").join("logs")),
        special_confirm: false,
        hint: "",
    },
    TargetDef {
        id: "java_cache",
        group: Group::Apps,
        label: "Java deployment cache",
        desc: "Old Java applet/Web Start cache",
        tier: Tier::Safe,
        kind: Kind::Contents,
        paths: |e| one(e.local.join(r"Sun\Java\Deployment\cache")),
        special_confirm: false,
        hint: "",
    },
    TargetDef {
        id: "epic_webcache",
        group: Group::Apps,
        label: "Epic Games web cache",
        desc: "Launcher store pages cache — installed games untouched",
        tier: Tier::Safe,
        kind: Kind::Contents,
        paths: |e| {
            one(e
                .local
                .join("Epic Games Launcher")
                .join("Saved")
                .join("webcache"))
        },
        special_confirm: false,
        hint: "Close Epic Games Launcher first",
    },
    TargetDef {
        id: "postman_cache",
        group: Group::Apps,
        label: "Postman cache",
        desc: "Web cache — collections and workspaces untouched",
        tier: Tier::Safe,
        kind: Kind::Contents,
        paths: |e| one(e.roaming.join("Postman").join("Cache")),
        special_confirm: false,
        hint: "Close Postman first",
    },
];

/// Files-pattern helpers used by the clean core for `Kind::Files` targets.
pub fn file_pattern_for(def_id: &str) -> &'static str {
    match def_id {
        "memory_dmp" => "MEMORY.DMP",
        "prefetch" => "*.pf",
        "panther_logs" => "*.log",
        "cbs_logs" => "*.log",
        "thumbcache" => "thumbcache_*.db",
        _ => "*",
    }
}

/// Extra file patterns for the thumbnail target (icon cache lives next to it).
pub fn extra_file_patterns(def_id: &str) -> &'static [&'static str] {
    match def_id {
        "thumbcache" => &["iconcache_*.db"],
        _ => &[],
    }
}
