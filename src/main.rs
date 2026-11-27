//! Purge — neon-grade Windows junk cleaner built with Rust + egui.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(deprecated)]

mod app;
mod engine;
mod guard;
mod log;
mod platform;
mod settings;
mod theme;
mod ui;

use eframe::egui;

fn main() -> eframe::Result<()> {
    // Dev-run fallback: the release manifest already demands admin via UAC.
    #[cfg(windows)]
    if !platform::is_elevated() {
        if let Ok(exe) = std::env::current_exe() {
            let args: Vec<String> = std::env::args().skip(1).collect();
            let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
            let exe_str = exe.to_string_lossy().to_string();
            if platform::relaunch_elevated(&exe_str, &arg_refs) {
                return Ok(());
            }
        }
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Purge — Windows Cleaner")
            .with_inner_size([1240.0, 800.0])
            .with_min_inner_size([1000.0, 660.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Purge",
        options,
        Box::new(|cc| Ok(Box::new(app::PurgeApp::new(cc)))),
    )
}
