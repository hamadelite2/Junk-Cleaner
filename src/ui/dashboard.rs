//! Dashboard: live stats, disk bars, neon ring, quick actions.

use egui::Ui;

use super::widgets::{self, card};
use crate::app::{Page, PurgeApp};
use crate::engine::{fmt_size, paths::Env};
use crate::platform;
use crate::settings;
use crate::theme::Theme;

pub fn show(app: &mut PurgeApp, ui: &mut Ui, t: &Theme) {
    widgets::section_header(ui, "Dashboard", "System overview and quick actions", t);
    ui.add_space(6.0);

    // ---- stat row -----------------------------------------------------------
    let env = Env::detect();
    let disk = platform::disk_info(&env.system_drive);
    let (free_s, total_s, used_frac) = match disk {
        Some((total, free)) => (
            fmt_size(free),
            fmt_size(total),
            if total > 0 {
                (total - free) as f32 / total as f32
            } else {
                0.0
            },
        ),
        None => ("?".into(), "?".into(), 0.0),
    };
    let cleanable = app
        .scan_data
        .as_ref()
        .map(|s| s.selected_bytes())
        .unwrap_or(0);
    let lifetime = settings::get().lifetime_freed;
    let runs = settings::get().clean_runs;

    ui.horizontal(|ui| {
        let w = (ui.available_width() - 24.0) / 3.0;
        ui.allocate_ui(egui::vec2(w, 100.0), |ui| {
            widgets::stat_card(
                ui,
                "st-free",
                "Free on C:",
                &free_s,
                &format!("of {total_s} total"),
                t,
            );
        });
        ui.allocate_ui(egui::vec2(w, 100.0), |ui| {
            widgets::stat_card(
                ui,
                "st-clean",
                "Cleanable now",
                &fmt_size(cleanable),
                "selected targets",
                t,
            );
        });
        ui.allocate_ui(egui::vec2(w, 100.0), |ui| {
            widgets::stat_card(
                ui,
                "st-life",
                "Lifetime freed",
                &fmt_size(lifetime),
                &format!("{runs} clean runs"),
                t,
            );
        });
    });
    ui.add_space(10.0);

    // ---- ring + disk panel ----------------------------------------------------
    ui.horizontal(|ui| {
        // left: ring
        let ring_size = 190.0;
        let progress = {
            let scan_done = app.scan_data.is_some();
            let clean_frac = app.clean_progress_frac();
            if app.clean_running() {
                clean_frac
            } else if scan_done {
                1.0
            } else {
                0.0
            }
        };
        let selected_str = fmt_size(
            app.scan_data
                .as_ref()
                .map(|s| s.selected_bytes())
                .unwrap_or(0),
        );
        let (title, sub) = if app.clean_running() {
            ("CLEANING", "one moment")
        } else if app.scan_running() {
            ("SCANNING", "analyzing system")
        } else {
            (selected_str.as_str(), "selected junk")
        };
        widgets::neon_ring(ui, ring_size, progress, title, sub, t);

        ui.add_space(24.0);
        ui.vertical(|ui| {
            ui.set_min_width(ui.available_width() - 10.0);
            card(ui, "disk-card", t, true, |ui| {
                ui.label(
                    egui::RichText::new("SYSTEM DRIVE")
                        .size(10.5)
                        .strong()
                        .color(t.dim),
                );
                ui.add_space(4.0);
                widgets::shimmer_bar(ui, used_frac, t);
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(format!(
                        "{:.0}% used — {} free of {}",
                        used_frac * 100.0,
                        free_s,
                        total_s
                    ))
                    .size(11.5)
                    .color(t.dim),
                );
            });
            ui.add_space(8.0);
            card(ui, "scan-card", t, true, |ui| {
                let (label, color) = match app.scan_phase.as_deref() {
                    Some(p) => (p.to_string(), t.accent2),
                    None => (
                        match &app.scan_data {
                            Some(s) => format!(
                                "Scanned {} — {} orphan candidates",
                                s.scanned_at.format("%H:%M:%S"),
                                s.orphans.len()
                            ),
                            None => "No scan yet — press Rescan".to_string(),
                        },
                        t.ok,
                    ),
                };
                ui.horizontal(|ui| {
                    widgets::pulse_dot(ui, color, t);
                    ui.label(egui::RichText::new(label).size(11.5).color(t.dim));
                });
            });
        });
    });
    ui.add_space(14.0);

    // ---- quick actions --------------------------------------------------------
    ui.horizontal(|ui| {
        if widgets::grad_button(ui, "quick-clean", "Clean selected", t, false).clicked() {
            app.request_clean_from_selection();
        }
        ui.add_space(6.0);
        if widgets::ghost_button(ui, "quick-rescan", "Rescan", t).clicked() {
            app.spawn_scan();
        }
        ui.add_space(6.0);
        if widgets::ghost_button(ui, "quick-pause", "Pause browser updates", t).clicked() {
            app.page = Page::Browsers;
        }
    });
    ui.add_space(16.0);

    // ---- recent activity ------------------------------------------------------
    card(ui, "activity", t, false, |ui| {
        ui.label(
            egui::RichText::new("RECENT ACTIVITY")
                .size(10.5)
                .strong()
                .color(t.dim),
        );
        ui.add_space(6.0);
        let lines = crate::log::snapshot();
        let n = lines.len();
        let start = n.saturating_sub(7);
        for line in &lines[start..n] {
            let color = match line.level {
                crate::log::Level::Ok => t.ok,
                crate::log::Level::Warn => t.warn,
                crate::log::Level::Error => t.danger,
                crate::log::Level::Action => t.accent2,
                crate::log::Level::Info => t.dim,
            };
            ui.label(
                egui::RichText::new(format!("[{}] {}", line.time, line.text))
                    .size(11.0)
                    .color(color),
            );
        }
        if n == 0 {
            ui.label(
                egui::RichText::new("Nothing yet.")
                    .size(11.0)
                    .color(t.faint),
            );
        }
    });

    // cancel scanning repaint hint
    if app.scan_running() || app.clean_running() {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(60));
    }
}
