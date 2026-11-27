//! Settings page: safety, orphan strictness, appearance, data.

use egui::Ui;

use super::widgets::{self, card, toggle};
use crate::app::PurgeApp;
use crate::settings;
use crate::theme::{self, Accent, Theme};

pub fn show(app: &mut PurgeApp, ui: &mut Ui, t: &Theme) {
    widgets::section_header(ui, "Settings", "Behavior, safety nets and appearance", t);

    // ---------------- safety ----------------
    card(ui, "set-safety", t, false, |ui| {
        ui.label(
            egui::RichText::new("SAFETY")
                .size(10.5)
                .strong()
                .color(t.dim),
        );
        ui.add_space(6.0);

        let mut rp = settings::get().restore_point;
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("System restore point before every clean")
                    .size(12.5)
                    .color(t.text),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if toggle(ui, "set-rp", &mut rp, t).changed() {
                    settings::update(|s| s.restore_point = rp);
                }
            });
        });
        ui.label(
            egui::RichText::new("Recommended. Windows limits restore points to one per 24h.")
                .size(10.0)
                .color(t.faint),
        );

        ui.add_space(8.0);
        let mut rb = settings::get().recycle_bin_mode;
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Send deletions to Recycle Bin (slower, reversible)")
                    .size(12.5)
                    .color(t.text),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if toggle(ui, "set-rb", &mut rb, t).changed() {
                    settings::update(|s| s.recycle_bin_mode = rb);
                }
            });
        });

        ui.add_space(8.0);
        let mut cf = settings::get().confirm_before_clean;
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Confirm before every clean run")
                    .size(12.5)
                    .color(t.text),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if toggle(ui, "set-cf", &mut cf, t).changed() {
                    settings::update(|s| s.confirm_before_clean = cf);
                }
            });
        });
    });
    ui.add_space(10.0);

    // ---------------- orphan strictness ----------------
    card(ui, "set-orphan", t, false, |ui| {
        ui.label(
            egui::RichText::new("ORPHAN DETECTION")
                .size(10.5)
                .strong()
                .color(t.dim),
        );
        ui.add_space(6.0);
        let mut days = settings::get().orphan_age_days as f64;
        ui.add(
            egui::Slider::new(&mut days, 14.0..=365.0)
                .text("folders must be idle at least (days)")
                .integer(),
        );
        if (days as u32) != settings::get().orphan_age_days {
            settings::update(|s| s.orphan_age_days = days as u32);
        }
        ui.label(
            egui::RichText::new(
                "Higher = safer. A folder is only proposed when EVERY identity source (installed programs, Store apps, Start Menu, processes, services) agrees it is unknown AND it has been idle this long.",
            )
            .size(10.5)
            .color(t.faint),
        );
    });
    ui.add_space(10.0);

    // ---------------- appearance ----------------
    card(ui, "set-look", t, false, |ui| {
        ui.label(
            egui::RichText::new("APPEARANCE")
                .size(10.5)
                .strong()
                .color(t.dim),
        );
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            for acc in Accent::all() {
                let selected = settings::get().accent == acc;
                let label = acc.label();
                if widgets::ghost_button(ui, &format!("acc-{label}"), label, t).clicked() {
                    settings::update(|s| s.accent = acc);
                    app.theme = theme::theme(acc);
                }
                if selected {
                    widgets::pulse_dot(ui, t.accent2, t);
                }
                ui.add_space(4.0);
            }
        });
    });
    ui.add_space(10.0);

    // ---------------- data ----------------
    card(ui, "set-data", t, false, |ui| {
        ui.label(egui::RichText::new("DATA").size(10.5).strong().color(t.dim));
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if widgets::ghost_button(ui, "set-logs", "Open log folder", t).clicked() {
                let _ = std::fs::create_dir_all(crate::log::log_dir());
                let _ = open::that(crate::log::log_dir());
            }
            ui.add_space(6.0);
            if widgets::ghost_button(ui, "set-reset", "Reset lifetime stats", t).clicked() {
                settings::update(|s| {
                    s.lifetime_freed = 0;
                    s.clean_runs = 0;
                });
            }
            ui.add_space(6.0);
            if widgets::ghost_button(ui, "set-clearsvc", "Forget saved service states", t).clicked()
            {
                settings::update(|s| s.saved_service_starts.clear());
            }
        });
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(format!(
                "Settings file: {}",
                settings::settings_path().display()
            ))
            .size(10.0)
            .color(t.faint),
        );
        ui.label(
            egui::RichText::new(format!(
                "Log file: {}",
                crate::log::log_file_path()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "n/a".into())
            ))
            .size(10.0)
            .color(t.faint),
        );
    });
    ui.add_space(10.0);

    // ---------------- about ----------------
    card(ui, "set-about", t, false, |ui| {
        ui.label(
            egui::RichText::new("ABOUT")
                .size(10.5)
                .strong()
                .color(t.dim),
        );
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new(
                "Purge v1.0 — neon-grade Windows junk cleaner built in Rust + egui.",
            )
            .size(12.0)
            .color(t.text),
        );
        ui.label(
            egui::RichText::new(
                "Purge deletes files permanently by default. Cache targets are whitelist-only; orphan deletion requires per-item review. The authors accept no liability for data loss.",
            )
            .size(10.5)
            .color(t.warn),
        );
    });
}
