//! Package Managers page: detect, clean caches, full removal, official uninstallers.

use egui::Ui;

use super::widgets::{self, card};
use crate::app::PurgeApp;
use crate::engine::{fmt_size, packages};
use crate::theme::Theme;

pub fn show(app: &mut PurgeApp, ui: &mut Ui, t: &Theme) {
    widgets::section_header(
        ui,
        "Package Managers",
        "Detect and manage 25 package managers — clean caches or remove them entirely",
        t,
    );

    let Some(data) = app.scan_data.as_ref() else {
        card(ui, "pk-noscan", t, false, |ui| {
            ui.label(
                egui::RichText::new("Run a scan first to detect installed package managers.")
                    .size(12.5)
                    .color(t.dim),
            );
            ui.add_space(6.0);
            if widgets::grad_button(ui, "pk-scan", "Scan now", t, false).clicked() {
                app.spawn_scan();
            }
        });
        return;
    };

    let pkgs = data.packages.clone();
    let count = pkgs.len();
    let cache_total: u64 = pkgs.iter().map(|p| p.cache_size).sum();

    ui.horizontal(|ui| {
        widgets::chip(ui, &format!("{count} managers detected"), t.accent2, t);
        widgets::chip(
            ui,
            &format!("{} cache total", fmt_size(cache_total)),
            t.ok,
            t,
        );
    });
    ui.add_space(10.0);

    if pkgs.is_empty() {
        card(ui, "pk-none", t, false, |ui| {
            ui.label(
                egui::RichText::new("No package manager data found on this machine.")
                    .size(12.0)
                    .color(t.dim),
            );
        });
        return;
    }

    for (idx, p) in pkgs.iter().enumerate() {
        card(ui, &format!("pkg-{}", p.id), t, true, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.set_min_width(ui.available_width() * 0.62);
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(p.name)
                                .size(14.0)
                                .strong()
                                .color(t.text),
                        );
                        widgets::chip(ui, p.category, t.accent, t);
                        if p.removes_tool {
                            widgets::chip(ui, "REMovable", t.warn, t);
                        }
                        if p.exe_path.is_some() {
                            widgets::chip(ui, "ON PATH", t.ok, t);
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(format!(
                                "cache {}   ·   footprint {}",
                                fmt_size(p.cache_size),
                                fmt_size(p.full_size)
                            ))
                            .size(11.0)
                            .color(t.dim),
                        );
                    });
                    for (cp, sz) in p.cache_paths.iter().take(3) {
                        widgets::path_line(
                            ui,
                            &format!("{}  ({})", cp.display(), fmt_size(*sz)),
                            t,
                        );
                    }
                    if !p.note.is_empty() {
                        ui.label(
                            egui::RichText::new(p.note)
                                .size(10.0)
                                .color(t.faint)
                                .italics(),
                        );
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let has_cache = p.cache_size > 0;
                    if widgets::grad_button(ui, &format!("pk-clean-{idx}"), "Clean cache", t, false)
                        .clicked()
                    {
                        app.confirm_pkg_cache(p.clone());
                    }
                    ui.add_space(6.0);
                    if has_cache
                        && widgets::grad_button(
                            ui,
                            &format!("pk-remove-{idx}"),
                            "Remove all data",
                            t,
                            true,
                        )
                        .clicked()
                    {
                        app.confirm_pkg_remove(p.clone());
                    }
                    ui.add_space(6.0);
                    if widgets::ghost_button(ui, &format!("pk-details-{idx}"), "Details", t)
                        .clicked()
                    {
                        app.pkg_details_open = Some(p.id.to_string());
                    }
                });
            });

            // official uninstaller button when registered
            let uninstaller = packages::find_official_uninstaller(
                packages::MANAGERS
                    .iter()
                    .find(|d| d.id == p.id)
                    .map(|d| d.uninstall_names)
                    .unwrap_or(&[]),
            );
            if let Some((name, _us)) = uninstaller {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    widgets::pulse_dot(ui, t.ok, t);
                    ui.label(
                        egui::RichText::new(format!("Official uninstaller found: {name}"))
                            .size(10.5)
                            .color(t.dim),
                    );
                    if widgets::ghost_button(ui, &format!("pk-unins-{idx}"), "Run uninstaller", t)
                        .clicked()
                    {
                        app.run_official_uninstaller(p.id);
                    }
                });
            }
        });
        ui.add_space(10.0);
    }
}
