//! Deep Clean page: curated catalog groups + accurate orphan review list.

use egui::Ui;

use super::widgets::{self, card};
use crate::app::PurgeApp;
use crate::engine::{fmt_size, Group, Tier};
use crate::theme::Theme;

struct OrphanRow {
    name: String,
    path: String,
    size: u64,
    last_used: Option<std::time::SystemTime>,
    evidence: Vec<String>,
    selected: bool,
}

pub fn show(app: &mut PurgeApp, ui: &mut Ui, t: &Theme) {
    widgets::section_header(
        ui,
        "Deep Clean",
        "Whitelisted cache targets + reviewed leftovers of uninstalled apps",
        t,
    );

    if app.scan_data.is_none() {
        card(ui, "no-scan", t, false, |ui| {
            ui.label(
                egui::RichText::new("Run a scan first to discover cleanable targets.")
                    .size(12.5)
                    .color(t.dim),
            );
            ui.add_space(6.0);
            if widgets::grad_button(ui, "dc-scan", "Scan now", t, false).clicked() {
                app.spawn_scan();
            }
        });
        return;
    }

    // ---------- action bar ----------
    ui.horizontal(|ui| {
        if widgets::ghost_button(ui, "dc-safe", "Select safe defaults", t).clicked() {
            if let Some(data) = app.scan_data.as_mut() {
                for it in data.items.iter_mut() {
                    it.selected = it.tier == Tier::Safe && !it.special_confirm;
                }
                for o in data.orphans.iter_mut() {
                    o.selected = false;
                }
            }
        }
        ui.add_space(4.0);
        if widgets::ghost_button(ui, "dc-clear", "Clear all", t).clicked() {
            if let Some(data) = app.scan_data.as_mut() {
                for it in data.items.iter_mut() {
                    it.selected = false;
                }
                for o in data.orphans.iter_mut() {
                    o.selected = false;
                }
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let selected_bytes = app
                .scan_data
                .as_ref()
                .map(|d| d.selected_bytes())
                .unwrap_or(0);
            widgets::chip(
                ui,
                &format!("{} selected", fmt_size(selected_bytes)),
                t.accent2,
                t,
            );
            ui.add_space(8.0);
            if widgets::grad_button(ui, "dc-clean", "Clean now", t, false).clicked() {
                app.request_clean_from_selection();
            }
        });
    });
    ui.add_space(12.0);

    // ---------- catalog groups ----------
    let groups_index: Vec<(Group, Vec<usize>)> = Group::all()
        .iter()
        .map(|&g| {
            (
                g,
                app.scan_data
                    .as_ref()
                    .map(|d| {
                        d.items
                            .iter()
                            .enumerate()
                            .filter(|(_, i)| i.group == g)
                            .map(|(idx, _)| idx)
                            .collect()
                    })
                    .unwrap_or_default(),
            )
        })
        .collect();

    for (group, group_items) in groups_index {
        if group_items.is_empty() {
            continue;
        }
        let (total, selected, all_selected) = {
            let data = app.scan_data.as_ref().unwrap();
            let total = group_items
                .iter()
                .filter_map(|&i| data.items.get(i).map(|it| it.size))
                .sum();
            let selected = group_items
                .iter()
                .filter_map(|&i| data.items.get(i))
                .filter(|it| it.selected)
                .map(|it| it.size)
                .sum();
            let all = group_items
                .iter()
                .all(|&i| data.items.get(i).map(|it| it.selected).unwrap_or(false));
            (total, selected, all)
        };

        card(ui, &format!("grp-{:?}", group), t, false, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new(group.title())
                            .size(14.5)
                            .strong()
                            .color(t.text),
                    );
                    ui.label(
                        egui::RichText::new(group.subtitle())
                            .size(10.5)
                            .color(t.faint),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let mut all = all_selected;
                    if widgets::toggle(ui, &format!("grp-tgl-{:?}", group), &mut all, t).changed() {
                        let data = app.scan_data.as_mut().unwrap();
                        for &i in &group_items {
                            if let Some(it) = data.items.get_mut(i) {
                                if it.special_confirm && all {
                                    // never bulk-select destructive specials
                                    continue;
                                }
                                it.selected = all;
                            }
                        }
                    }
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new(format!(
                            "{} / {}",
                            fmt_size(selected),
                            fmt_size(total)
                        ))
                        .size(12.0)
                        .color(t.dim),
                    );
                });
            });

            ui.add_space(6.0);
            for &i in &group_items {
                let (label, desc, hint, tier, size, special, def_id, selected_now) = {
                    let it = &app.scan_data.as_ref().unwrap().items[i];
                    (
                        it.label.clone(),
                        it.desc.clone(),
                        it.hint.clone(),
                        it.tier,
                        it.size,
                        it.special_confirm,
                        it.def_id,
                        it.selected,
                    )
                };
                ui.add_space(3.0);
                ui.horizontal(|ui| {
                    let mut sel = selected_now;
                    if widgets::checkbox(ui, &format!("it-{def_id}"), &mut sel, t).changed() {
                        app.scan_data.as_mut().unwrap().items[i].selected = sel;
                    }
                    ui.vertical(|ui| {
                        ui.set_min_width(ui.available_width() - 8.0);
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(&label)
                                    .size(12.5)
                                    .color(t.text)
                                    .strong(),
                            );
                            let tier_color = match tier {
                                Tier::Safe => t.ok,
                                Tier::Moderate => t.warn,
                            };
                            widgets::chip(ui, tier.label(), tier_color, t);
                            if !hint.is_empty() {
                                widgets::chip(ui, hint.as_str(), t.warn, t);
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(
                                        egui::RichText::new(fmt_size(size))
                                            .size(12.0)
                                            .color(t.accent2)
                                            .strong(),
                                    );
                                },
                            );
                        });
                        ui.label(egui::RichText::new(&desc).size(10.5).color(t.dim));
                        if special {
                            ui.label(
                                egui::RichText::new(
                                    "Requires explicit confirmation in the dialog before cleaning.",
                                )
                                .size(10.0)
                                .color(t.danger),
                            );
                        }
                    });
                });
            }
        });
        ui.add_space(10.0);
    }

    // ---------- orphan review ----------
    let orphan_count = app.scan_data.as_ref().map(|d| d.orphans.len()).unwrap_or(0);
    let token_count = app.scan_data.as_ref().map(|d| d.token_count).unwrap_or(0);

    if orphan_count > 0 {
        card(ui, "orphans", t, false, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Leftovers of uninstalled apps")
                        .size(14.5)
                        .strong()
                        .color(t.text),
                );
                widgets::chip(ui, "REVIEW REQUIRED", t.warn, t);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let total = app
                        .scan_data
                        .as_ref()
                        .map(|d| d.orphans.iter().map(|o| o.size).sum())
                        .unwrap_or(0);
                    ui.label(egui::RichText::new(fmt_size(total)).size(12.0).color(t.dim));
                });
            });
            ui.label(
                egui::RichText::new(format!(
                    "Every folder below failed ALL {} identity checks (installed programs, Store apps, program folders, Start Menu, running processes, services) and has been idle for at least {} days. Game saves or settings may still live here — review before deleting.",
                    token_count,
                    crate::settings::get().orphan_age_days
                ))
                .size(10.5)
                .color(t.warn),
            );
            ui.add_space(8.0);

            let orphans: Vec<OrphanRow> = app
                .scan_data
                .as_ref()
                .unwrap()
                .orphans
                .iter()
                .map(|o| OrphanRow {
                    name: o.name.clone(),
                    path: o.path.display().to_string(),
                    size: o.size,
                    last_used: o.last_used,
                    evidence: o.evidence.clone(),
                    selected: o.selected,
                })
                .collect();

            for (oi, row) in orphans.iter().enumerate() {
                let (name, path, size, last_used, evidence, selected_now) = (
                    &row.name,
                    &row.path,
                    row.size,
                    row.last_used,
                    &row.evidence,
                    row.selected,
                );
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    let mut sel = selected_now;
                    if widgets::checkbox(ui, &format!("or-{oi}"), &mut sel, t).changed() {
                        app.scan_data.as_mut().unwrap().orphans[oi].selected = sel;
                    }
                    ui.vertical(|ui| {
                        ui.set_min_width(ui.available_width() - 8.0);
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(name).size(12.5).strong().color(t.text));
                            let age = last_used
                                .map(|l| {
                                    let d = std::time::SystemTime::now()
                                        .duration_since(l)
                                        .map(|d| d.as_secs() / 86400)
                                        .unwrap_or(0);
                                    format!("idle {} days", d)
                                })
                                .unwrap_or_default();
                            if !age.is_empty() {
                                widgets::chip(ui, &age, t.dim, t);
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(
                                        egui::RichText::new(fmt_size(size))
                                            .size(12.0)
                                            .color(t.accent2)
                                            .strong(),
                                    );
                                },
                            );
                        });
                        widgets::path_line(ui, path, t);
                        for ev in evidence {
                            ui.label(
                                egui::RichText::new(format!("— {ev}"))
                                    .size(10.0)
                                    .color(t.faint),
                            );
                        }
                    });
                });
            }
        });
    } else if orphan_count == 0 && !app.scan_data.as_ref().unwrap().items.is_empty() {
        card(ui, "orphans-empty", t, false, |ui| {
            ui.horizontal(|ui| {
                widgets::pulse_dot(ui, t.ok, t);
                ui.label(
                    egui::RichText::new(
                        "No leftovers of uninstalled apps found — your AppData/ProgramData is clean.",
                    )
                    .size(11.5)
                    .color(t.dim),
                );
            });
        });
    }
}
