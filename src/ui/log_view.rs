//! Activity log page.

use egui::ScrollArea;
use egui::Ui;

use super::widgets::{self, card};
use crate::app::PurgeApp;
use crate::log::Level;
use crate::theme::Theme;

pub fn show(_app: &mut PurgeApp, ui: &mut Ui, t: &Theme) {
    widgets::section_header(ui, "Activity Log", "Everything Purge did this session", t);

    ui.horizontal(|ui| {
        if widgets::ghost_button(ui, "log-open", "Open log folder", t).clicked() {
            let _ = std::fs::create_dir_all(crate::log::log_dir());
            let _ = open::that(crate::log::log_dir());
        }
        ui.add_space(6.0);
        if widgets::ghost_button(ui, "log-open-file", "Open current log", t).clicked() {
            if let Some(p) = crate::log::log_file_path() {
                let _ = open::that(p);
            }
        }
    });
    ui.add_space(10.0);

    card(ui, "log-card", t, false, |ui| {
        ScrollArea::vertical()
            .auto_shrink(false)
            .stick_to_bottom(true)
            .max_height(ui.available_height().max(300.0))
            .show(ui, |ui| {
                let lines = crate::log::snapshot();
                for l in &lines {
                    let color = match l.level {
                        Level::Ok => t.ok,
                        Level::Warn => t.warn,
                        Level::Error => t.danger,
                        Level::Action => t.accent2,
                        Level::Info => t.dim,
                    };
                    ui.label(
                        egui::RichText::new(format!("{}  {}  {}", l.time, l.level.tag(), l.text))
                            .size(11.0)
                            .color(color),
                    );
                }
                if lines.is_empty() {
                    ui.label(
                        egui::RichText::new("Log is empty.")
                            .size(11.5)
                            .color(t.faint),
                    );
                }
            });
    });
}
