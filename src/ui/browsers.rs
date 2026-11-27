//! Browsers page: pause/resume automatic updates + cache cleaning per browser.

use egui::Ui;

use super::widgets::{self, card};
use crate::app::PurgeApp;
use crate::engine::browsers::PauseState;
use crate::engine::fmt_size;
use crate::theme::Theme;

fn state_color(state: PauseState, t: &Theme) -> egui::Color32 {
    match state {
        PauseState::Active => t.accent2,
        PauseState::Partial => t.warn,
        PauseState::Paused => t.ok,
    }
}

pub fn show(app: &mut PurgeApp, ui: &mut Ui, t: &Theme) {
    widgets::section_header(
        ui,
        "Browser Updates",
        "Pause automatic updates (tasks + services + official policies) and clean caches",
        t,
    );

    let Some(data) = app.scan_data.as_ref() else {
        card(ui, "br-noscan", t, false, |ui| {
            ui.label(
                egui::RichText::new("Run a scan first to detect installed browsers.")
                    .size(12.5)
                    .color(t.dim),
            );
            ui.add_space(6.0);
            if widgets::grad_button(ui, "br-scan", "Scan now", t, false).clicked() {
                app.spawn_scan();
            }
        });
        return;
    };

    // refresh live states from the actual system (authoritative)
    let browsers = {
        let mut v = data.browsers.clone();
        for b in v.iter_mut() {
            b.pause = crate::engine::browsers::live_state(b.id);
        }
        v
    };

    // master controls
    card(ui, "br-master", t, false, |ui| {
        ui.label(
            egui::RichText::new("MASTER CONTROLS")
                .size(10.5)
                .strong()
                .color(t.dim),
        );
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if widgets::grad_button(ui, "br-pause-all", "Pause all updates", t, false).clicked() {
                app.confirm_pause_all(&browsers);
            }
            ui.add_space(6.0);
            if widgets::ghost_button(ui, "br-resume-all", "Resume all updates", t).clicked() {
                app.confirm_resume_all(&browsers);
            }
            ui.add_space(6.0);
            if widgets::grad_button(ui, "br-pause-clean", "Pause + clean caches", t, false)
                .clicked()
            {
                app.confirm_pause_and_clean(&browsers);
            }
        });
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(
                "Pausing writes official vendor policies (Chrome/Edge/Brave/Firefox Update policies), disables updater scheduled tasks and services. Resume restores everything exactly.",
            )
            .size(10.5)
            .color(t.faint),
        );
    });
    ui.add_space(10.0);

    if browsers.is_empty() {
        card(ui, "br-none", t, false, |ui| {
            ui.label(
                egui::RichText::new("No browsers detected.")
                    .size(12.0)
                    .color(t.dim),
            );
        });
        return;
    }

    for (idx, b) in browsers.iter().enumerate() {
        let paused_now = b.pause == PauseState::Paused;
        card(ui, &format!("br-{}", b.id), t, true, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.set_min_width(ui.available_width() * 0.6);
                    ui.horizontal(|ui| {
                        widgets::pulse_dot(ui, state_color(b.pause, t), t);
                        ui.label(
                            egui::RichText::new(b.name).size(14.0).strong().color(t.text),
                        );
                        widgets::chip(ui, b.pause.label(), state_color(b.pause, t), t);
                    });
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(format!(
                                "cache {}  ·  {} updater task(s)  ·  {} updater service(s)",
                                fmt_size(b.cache_size),
                                b.task_hits.len(),
                                b.service_hits.len()
                            ))
                            .size(11.0)
                            .color(t.dim),
                        );
                    });
                    for (cp, sz) in b.cache_paths.iter().take(2) {
                        widgets::path_line(ui, &format!("{}  ({})", cp.display(), fmt_size(*sz)), t);
                    }
                    if b.task_hits.is_empty() && b.service_hits.is_empty() {
                        ui.label(
                            egui::RichText::new(
                                "Update control via registry policy only (no tasks/services found).",
                            )
                            .size(10.0)
                            .color(t.faint),
                        );
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let act = if paused_now { "Resume updates" } else { "Pause updates" };
                    if widgets::grad_button(
                        ui,
                        &format!("br-btn-{idx}"),
                        act,
                        t,
                        paused_now,
                    )
                    .clicked()
                    {
                        if paused_now {
                            app.confirm_resume(b.id);
                        } else {
                            app.confirm_pause(b.id);
                        }
                    }
                    ui.add_space(6.0);
                    if widgets::ghost_button(ui, &format!("br-cache-{idx}"), "Clean cache", t)
                        .clicked()
                    {
                        app.confirm_browser_cache(b.id, b.cache_size);
                    }
                });
            });
        });
        ui.add_space(10.0);
    }
}
