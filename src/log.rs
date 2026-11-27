//! Session logging: in-memory ring buffer for the Log page + append-only file on disk.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Info,
    Ok,
    Warn,
    Error,
    Action,
}

impl Level {
    pub fn tag(&self) -> &'static str {
        match self {
            Level::Info => "INFO ",
            Level::Ok => " OK  ",
            Level::Warn => "WARN ",
            Level::Error => "ERROR",
            Level::Action => "ACT  ",
        }
    }
}

#[derive(Clone, Debug)]
pub struct LogLine {
    pub time: String,
    pub level: Level,
    pub text: String,
}

struct Logger {
    lines: Vec<LogLine>,
    file: Option<File>,
    path: Option<PathBuf>,
}

static LOGGER: OnceLock<Mutex<Logger>> = OnceLock::new();

fn logger() -> &'static Mutex<Logger> {
    LOGGER.get_or_init(|| {
        let dir = log_dir();
        let _ = fs::create_dir_all(&dir);
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
        let path = dir.join(format!("session-{stamp}.log"));
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .ok();
        Mutex::new(Logger {
            lines: Vec::new(),
            file,
            path: Some(path),
        })
    })
}

pub fn log_dir() -> PathBuf {
    let base = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("Purge").join("logs")
}

pub fn log_file_path() -> Option<PathBuf> {
    logger().lock().ok().and_then(|l| l.path.clone())
}

pub fn log(level: Level, text: impl Into<String>) {
    let text = text.into();
    let time = chrono::Local::now().format("%H:%M:%S").to_string();
    if let Ok(mut l) = logger().lock() {
        l.lines.push(LogLine {
            time,
            level,
            text: text.clone(),
        });
        // cap the in-memory buffer
        if l.lines.len() > 4000 {
            l.lines.drain(0..1000);
        }
        if let Some(f) = l.file.as_mut() {
            let stamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
            let _ = writeln!(f, "[{stamp}] {} {text}", level.tag());
        }
    }
}

pub fn info(text: impl Into<String>) {
    log(Level::Info, text);
}
pub fn ok(text: impl Into<String>) {
    log(Level::Ok, text);
}
pub fn warn(text: impl Into<String>) {
    log(Level::Warn, text);
}
pub fn error(text: impl Into<String>) {
    log(Level::Error, text);
}
pub fn action(text: impl Into<String>) {
    log(Level::Action, text);
}

pub fn snapshot() -> Vec<LogLine> {
    logger().lock().map(|l| l.lines.clone()).unwrap_or_default()
}
