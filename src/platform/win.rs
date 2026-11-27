//! Windows platform helpers: elevation, registry, scheduled tasks, services,
//! PATH lookup and PowerShell invocation. All admin-grade operations live here.

use std::collections::HashSet;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
use winreg::RegKey;

// ---------------------------------------------------------------- wide strings

pub fn wide(s: &str) -> Vec<u16> {
    OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

// ------------------------------------------------------------------ elevation

/// True when running with an elevated (admin) token.
pub fn is_elevated() -> bool {
    // SAFETY: IsUserAnAdmin reads the current process token; no side effects.
    unsafe { windows_sys::Win32::UI::Shell::IsUserAnAdmin() != 0 }
}

/// Relaunch this executable with the "runas" verb (UAC prompt). Returns false on failure.
pub fn relaunch_elevated(exe: &str, args: &[&str]) -> bool {
    let file = wide(exe);
    let params = wide(&args.join(" "));
    // SAFETY: ShellExecuteW with valid NUL-terminated wide strings.
    let result = unsafe {
        windows_sys::Win32::UI::Shell::ShellExecuteW(
            std::ptr::null_mut(),
            wide("runas").as_ptr(),
            file.as_ptr(),
            params.as_ptr(),
            std::ptr::null(),
            windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
        )
    };
    (result as isize) > 32
}

// ------------------------------------------------------------------- registry

#[derive(Clone, Copy, Debug)]
pub enum Hive {
    Hkcu,
    Hklm,
}

impl Hive {
    fn predef(self) -> winreg::HKEY {
        match self {
            Hive::Hkcu => HKEY_CURRENT_USER,
            Hive::Hklm => HKEY_LOCAL_MACHINE,
        }
    }
}

fn open_key(hive: Hive, path: &str) -> Option<RegKey> {
    RegKey::predef(hive.predef()).open_subkey(path).ok()
}

pub fn reg_read_u32(hive: Hive, path: &str, value: &str) -> Option<u32> {
    open_key(hive, path).and_then(|k| k.get_value::<u32, _>(value).ok())
}

pub fn reg_read_string(hive: Hive, path: &str, value: &str) -> Option<String> {
    open_key(hive, path).and_then(|k| k.get_value::<String, _>(value).ok())
}

pub fn reg_write_u32(hive: Hive, path: &str, value: &str, data: u32) -> Result<(), String> {
    let root = RegKey::predef(hive.predef());
    let key = root
        .create_subkey(path)
        .map_err(|e| format!("registry create {path}: {e}"))?
        .0;
    key.set_value(value, &data)
        .map_err(|e| format!("registry set {value}: {e}"))
}

pub fn reg_write_string(hive: Hive, path: &str, value: &str, data: &str) -> Result<(), String> {
    let root = RegKey::predef(hive.predef());
    let key = root
        .create_subkey(path)
        .map_err(|e| format!("registry create {path}: {e}"))?
        .0;
    key.set_value(value, &data)
        .map_err(|e| format!("registry set {value}: {e}"))
}

pub fn reg_delete_value(hive: Hive, path: &str, value: &str) -> Result<(), String> {
    if let Some(k) = open_key(hive, path) {
        k.delete_value(value)
            .map_err(|e| format!("registry delete {value}: {e}"))?;
    }
    Ok(())
}

/// List subkey names of a registry path.
pub fn reg_enum_subkeys(hive: Hive, path: &str) -> Vec<String> {
    open_key(hive, path)
        .map(|k| k.enum_keys().filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
}

/// Read string values from every subkey of `path` (used for uninstall enumeration).
pub fn reg_enum_with_values(
    hive: Hive,
    path: &str,
    values: &[&str],
) -> Vec<(String, Vec<Option<String>>)> {
    let mut out = Vec::new();
    if let Some(k) = open_key(hive, path) {
        for sub in k.enum_keys().filter_map(|r| r.ok()) {
            if let Ok(sk) = k.open_subkey(&sub) {
                let vals = values
                    .iter()
                    .map(|v| sk.get_value::<String, _>(v).ok())
                    .collect();
                out.push((sub, vals));
            }
        }
    }
    out
}

// ---------------------------------------------------------------- process runs

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn run(cmd: &mut Command) -> Result<Output, String> {
    // never flash a console window from a GUI app
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd.output()
        .map_err(|e| format!("spawn {:?}: {e}", cmd.get_program()))
}

/// Status line of one scheduled task ("Ready", "Disabled", ...).
pub fn schtasks_state(task_name: &str) -> Option<String> {
    let out = run(Command::new("schtasks").args(["/Query", "/TN", task_name, "/FO", "CSV", "/NH"]))
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    // "TaskName","Next Run Time","Status",...
    for line in text.lines() {
        let parts: Vec<&str> = line
            .split(',')
            .map(|p| p.trim().trim_matches('"'))
            .collect();
        if parts.len() >= 3 {
            return Some(parts[2].to_string());
        }
    }
    None
}

// ------------------------------------------------------------- scheduled tasks

/// All scheduled task names on the machine (path-prefixed, like \TaskName).
pub fn schtasks_names() -> Vec<String> {
    match run(Command::new("schtasks").args(["/Query", "/FO", "CSV", "/NH"])) {
        Ok(o) if o.status.success() => {
            let text = String::from_utf8_lossy(&o.stdout);
            let mut names = Vec::new();
            for line in text.lines() {
                if let Some(first) = line.split(',').next() {
                    let n = first.trim().trim_matches('"').trim().to_string();
                    if !n.is_empty() {
                        names.push(n);
                    }
                }
            }
            names
        }
        _ => Vec::new(),
    }
}

pub fn schtasks_set_enabled(task_name: &str, enabled: bool) -> Result<(), String> {
    let flag = if enabled { "/ENABLE" } else { "/DISABLE" };
    match run(Command::new("schtasks").args(["/Change", "/TN", task_name, flag])) {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => Err(format!(
            "schtasks {flag} {task_name}: {}",
            String::from_utf8_lossy(&o.stderr).trim()
        )),
        Err(e) => Err(e),
    }
}

// ------------------------------------------------------------------- services

/// Query a service start type: "auto", "auto-delayed", "demand", "disabled".
pub fn service_start_type(service: &str) -> Option<String> {
    let out = run(Command::new("sc").args(["qc", service])).ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("START_TYPE") {
            let v = line.split_whitespace().last()?.to_lowercase();
            return Some(v);
        }
    }
    None
}

pub fn service_set_start(service: &str, start_type: &str) -> Result<(), String> {
    match run(Command::new("sc").args(["config", service, "start=", start_type])) {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => Err(format!(
            "sc config {service}: {}",
            String::from_utf8_lossy(&o.stderr).trim()
        )),
        Err(e) => Err(e),
    }
}

pub fn service_stop(service: &str) -> Result<(), String> {
    match run(Command::new("sc").args(["stop", service])) {
        Ok(o) => {
            let code = o.status.code().unwrap_or(0);
            // 1060 = not installed, 1062 = not started: both fine for our purposes
            if code == 0 || code == 1060 || code == 1062 {
                Ok(())
            } else {
                Err(format!(
                    "sc stop {service}: {}",
                    String::from_utf8_lossy(&o.stderr).trim()
                ))
            }
        }
        Err(e) => Err(e),
    }
}

pub fn service_start(service: &str) -> Result<(), String> {
    match run(Command::new("sc").args(["start", service])) {
        Ok(o) => {
            let code = o.status.code().unwrap_or(0);
            if code == 0 || code == 1056 {
                Ok(())
            } else {
                Err(format!(
                    "sc start {service}: {}",
                    String::from_utf8_lossy(&o.stderr).trim()
                ))
            }
        }
        Err(e) => Err(e),
    }
}

/// All service names registered in the SCM database.
pub fn service_names() -> HashSet<String> {
    reg_enum_subkeys(Hive::Hklm, r"SYSTEM\CurrentControlSet\Services")
        .into_iter()
        .collect()
}

// ------------------------------------------------------------------ PATH scan

/// Locate an executable on PATH without spawning processes.
pub fn find_on_path(exe: &str) -> Option<PathBuf> {
    let path = std::env::var("PATH").unwrap_or_default();
    for dir in std::env::split_paths(&path) {
        for candidate in [
            dir.join(exe),
            dir.join(format!("{exe}.exe")),
            dir.join(format!("{exe}.cmd")),
            dir.join(format!("{exe}.bat")),
        ] {
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

// ----------------------------------------------------------------- powershell

pub fn powershell(script: &str) -> Result<String, String> {
    let out = run(Command::new("powershell").args([
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        script,
    ]))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

// ------------------------------------------------------------------ disk info

/// (total_bytes, free_bytes) for a drive root like C:\.
pub fn disk_info(root: &Path) -> Option<(u64, u64)> {
    use sysinfo::Disks;
    let disks = Disks::new_with_refreshed_list().ok()?;
    let root_prefix: String = root
        .to_string_lossy()
        .chars()
        .take(2)
        .collect::<String>()
        .to_lowercase();
    for d in &disks {
        let mp = d.mount_point().to_string_lossy().to_lowercase();
        if mp == root_prefix || mp.starts_with(&root_prefix) {
            return Some((d.total_space(), d.available_space()));
        }
    }
    None
}

// ---------------------------------------------------------- process snapshot

pub fn running_process_names() -> HashSet<String> {
    use sysinfo::{ProcessesToUpdate, System};
    let mut sys = match System::new() {
        Ok(s) => s,
        Err(_) => return HashSet::new(),
    };
    let _ = sys.refresh_processes(ProcessesToUpdate::All, true);
    sys.processes()
        .values()
        .filter_map(|p| {
            let name = p.name().to_string_lossy().to_lowercase();
            let stem = name.strip_suffix(".exe").unwrap_or(&name).to_string();
            if stem.is_empty() {
                None
            } else {
                Some(stem)
            }
        })
        .collect()
}
