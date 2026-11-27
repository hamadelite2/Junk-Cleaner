//! Non-Windows fallbacks with identical signatures so the crate type-checks
//! everywhere. The shipped product targets Windows; these stubs exist so
//! `cargo check` works during development on other hosts.

#![allow(dead_code)]

use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub fn is_elevated() -> bool {
    false
}

pub fn relaunch_elevated(_exe: &str, _args: &[&str]) -> bool {
    false
}

#[derive(Clone, Copy, Debug)]
pub enum Hive {
    Hkcu,
    Hklm,
}

pub fn reg_read_u32(_h: Hive, _p: &str, _v: &str) -> Option<u32> {
    None
}

pub fn reg_read_string(_h: Hive, _p: &str, _v: &str) -> Option<String> {
    None
}

pub fn reg_write_u32(_h: Hive, _p: &str, _v: &str, _d: u32) -> Result<(), String> {
    Err("windows only".into())
}

pub fn reg_write_string(_h: Hive, _p: &str, _v: &str, _d: &str) -> Result<(), String> {
    Err("windows only".into())
}

pub fn reg_delete_value(_h: Hive, _p: &str, _v: &str) -> Result<(), String> {
    Ok(())
}

pub fn reg_enum_subkeys(_h: Hive, _p: &str) -> Vec<String> {
    Vec::new()
}

pub fn reg_enum_with_values(_h: Hive, _p: &str, _v: &[&str]) -> Vec<(String, Vec<Option<String>>)> {
    Vec::new()
}

pub fn schtasks_names() -> Vec<String> {
    Vec::new()
}

pub fn schtasks_set_enabled(_t: &str, _e: bool) -> Result<(), String> {
    Err("windows only".into())
}

pub fn schtasks_state(_t: &str) -> Option<String> {
    None
}

pub fn service_start_type(_s: &str) -> Option<String> {
    None
}

pub fn service_set_start(_s: &str, _t: &str) -> Result<(), String> {
    Err("windows only".into())
}

pub fn service_stop(_s: &str) -> Result<(), String> {
    Err("windows only".into())
}

pub fn service_start(_s: &str) -> Result<(), String> {
    Err("windows only".into())
}

pub fn service_names() -> HashSet<String> {
    HashSet::new()
}

pub fn find_on_path(_exe: &str) -> Option<PathBuf> {
    None
}

pub fn powershell(_s: &str) -> Result<String, String> {
    Err("windows only".into())
}

pub fn disk_info(_root: &Path) -> Option<(u64, u64)> {
    None
}

pub fn running_process_names() -> HashSet<String> {
    HashSet::new()
}
