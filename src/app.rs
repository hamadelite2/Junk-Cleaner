//! Application state, navigation, modals and worker-thread wiring.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;

use egui::{Align, Align2, Color32, Id, Layout, Pos2, Rect, Sense, Vec2};

use crate::engine::browsers::{self, BrowserInfo};
use crate::engine::clean::{run_clean, CleanPlan};
use crate::engine::packages::{self, PkgInfo};
use crate::engine::paths::Env;
use crate::engine::scan::run_scan;
use crate::engine::{CleanSummary, Event, Progress, ScanResult};
use crate::guard::Guard;
use crate::log;
use crate::settings;
use crate::theme::{self, Theme};
use crate::ui::{
    browsers as br_page, dashboard, deep_clean, log_view, packages as pk_page,
    settings as set_page,
    widgets::{self, Burst},
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Page {
    Dashboard,
    DeepClean,
    Packages,
    Browsers,
    Settings,
    Log,
}

impl Page {
    pub fn label(&self) -> &'static str {
        match self {
            Page::Dashboard => "Dashboard",
            Page::DeepClean => "Deep Clean",
            Page::Packages => "Package Managers",
            Page::Browsers => "Browser Updates",
            Page::Settings => "Settings",
            Page::Log => "Activity Log",
        }
    }

    pub fn all() -> [Page; 6] {
        [
            Page::Dashboard,
            Page::DeepClean,
            Page::Packages,
            Page::Browsers,
            Page::Settings,
            Page::Log,
        ]
    }
}

#[derive(Clone, Debug)]
pub enum PendingAction {
    RunClean(CleanPlan),
    ResumeBrowsers(Vec<String>),
    PkgUninstall { id: String, display: String },
}

pub struct ConfirmDialog {
    pub title: String,
    pub lines: Vec<String>,
    pub danger: bool,
    pub action: PendingAction,
}

pub struct PurgeApp {
    pub page: Page,
    pub theme: Theme,
    rx: Receiver<Event>,
    tx: Sender<Event>,
    pub scan_data: Option<ScanResult>,
    pub scan_phase: Option<String>,
    pub scan_running: bool,
    // clean live state
    pub clean_phase: Option<String>,
    pub clean_current: Option<String>,
    pub clean_freed: u64,
    pub clean_total: u64,
    pub clean_running: bool,
    pub summary_open: Option<CleanSummary>,
    pub burst_pending: bool,
    pub bursts: Vec<Burst>,
    pub confirm: Option<ConfirmDialog>,
    pub pkg_details_open: Option<String>,
    pub fade_gen: u32,
    cancel_scan: Arc<AtomicBool>,
    cancel_clean: Arc<AtomicBool>,
}

impl PurgeApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let settings = settings::Settings::load();
        settings::init(settings);
        let accent = settings::get().accent;
        let theme = theme::apply(&cc.egui_ctx, accent);

        let (tx, rx) = mpsc::channel();
        let app = PurgeApp {
            page: Page::Dashboard,
            theme,
            rx,
            tx,
            scan_data: None,
            scan_phase: None,
            scan_running: false,
            clean_phase: None,
            clean_current: None,
            clean_freed: 0,
            clean_total: 0,
            clean_running: false,
            summary_open: None,
            burst_pending: false,
            bursts: Vec::new(),
            confirm: None,
            pkg_details_open: None,
            fade_gen: 0,
            cancel_scan: Arc::new(AtomicBool::new(false)),
            cancel_clean: Arc::new(AtomicBool::new(false)),
        };
        log::info("Purge started");
        if !crate::platform::is_elevated() {
            log::warn("Not running as administrator — some cleanup targets and update controls will fail. Run as admin for full power.");
        }
        app
    }

    // ------------------------------------------------------------ job spawning

    pub fn spawn_scan(&mut self) {
        if self.scan_running {
            return;
        }
        self.scan_running = true;
        self.scan_phase = Some("Starting scan...".into());
        self.cancel_scan = Arc::new(AtomicBool::new(false));
        let env = Env::detect();
        let age = settings::get().orphan_age_days;
        let tx = self.tx.clone();
        let cancel = self.cancel_scan.clone();
        log::action("Scan job spawned");
        std::thread::spawn(move || run_scan(env, age, tx, cancel));
    }

    pub fn spawn_clean(&mut self, plan: CleanPlan) {
        if self.clean_running {
            return;
        }
        // estimate planned bytes for the progress fraction
        let mut planned = 0u64;
        if let Some(data) = self.scan_data.as_ref() {
            for i in data
                .items
                .iter()
                .filter(|i| plan.catalog.iter().any(|id| id == i.def_id))
            {
                planned += i.size;
            }
            for o in data
                .orphans
                .iter()
                .filter(|o| plan.orphans.iter().any(|p| p.as_path() == o.path.as_path()))
            {
                planned += o.size;
            }
        }
        planned += plan.orphans.len() as u64 * 50_000_000; // rough guess when no scan data
        self.clean_total = planned.max(1);
        self.clean_freed = 0;
        self.clean_phase = Some("Preparing...".into());
        self.clean_current = None;
        self.clean_running = true;
        self.cancel_clean = Arc::new(AtomicBool::new(false));
        let env = Env::detect();
        let s = settings::get();
        let tx = self.tx.clone();
        let cancel = self.cancel_clean.clone();
        let guard = Arc::new(Guard::new());
        log::action("Clean job spawned");
        std::thread::spawn(move || run_clean(env, plan, s, guard, tx, cancel));
    }

    pub fn spawn_resume(&mut self, ids: Vec<String>) {
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            for id in ids {
                let msgs = browsers::resume_updates(&id);
                for m in msgs {
                    log::info(m);
                }
            }
            let _ = tx;
        });
    }

    // ------------------------------------------------------------ confirmations

    pub fn request_clean_from_selection(&mut self) {
        let Some(data) = self.scan_data.as_ref() else {
            return;
        };
        let catalog: Vec<String> = data
            .items
            .iter()
            .filter(|i| i.selected)
            .map(|i| i.def_id.to_string())
            .collect();
        let orphans: Vec<_> = data
            .orphans
            .iter()
            .filter(|o| o.selected)
            .map(|o| o.path.clone())
            .collect();
        if catalog.is_empty() && orphans.is_empty() {
            log::warn("Nothing selected to clean.");
            return;
        }
        let total = data.selected_bytes();
        let s = settings::get();
        let mut lines = vec![
            format!(
                "About to permanently delete {} of selected junk.",
                crate::engine::fmt_size(total)
            ),
            format!(
                "Restore point: {}",
                if s.restore_point {
                    "yes (if none in last 24h)"
                } else {
                    "disabled"
                }
            ),
            format!(
                "Deletion mode: {}",
                if s.recycle_bin_mode {
                    "Recycle Bin (reversible)"
                } else {
                    "PERMANENT"
                }
            ),
        ];
        let specials: Vec<&str> = data
            .items
            .iter()
            .filter(|i| i.selected && i.special_confirm)
            .map(|i| i.label.as_str())
            .collect();
        if !specials.is_empty() {
            lines.push(format!(
                "DESTRUCTIVE: {} — this cannot be undone.",
                specials.join(", ")
            ));
        }
        let moderates = data
            .items
            .iter()
            .filter(|i| i.selected && i.tier == crate::engine::Tier::Moderate)
            .count();
        if moderates > 0 {
            lines.push(format!("{moderates} moderate-tier target(s) included."));
        }
        if !orphans.is_empty() {
            lines.push(format!(
                "{} leftover folder(s) of uninstalled apps will be removed.",
                orphans.len()
            ));
        }
        self.confirm = Some(ConfirmDialog {
            title: "Run clean?".into(),
            lines,
            danger: !specials.is_empty(),
            action: PendingAction::RunClean(CleanPlan {
                catalog,
                orphans,
                pkg_cache: vec![],
                pkg_remove: vec![],
                browser_pause: vec![],
                browser_cache: vec![],
            }),
        });
    }

    pub fn confirm_pkg_cache(&mut self, p: PkgInfo) {
        let lines = vec![
            format!(
                "Clean cache of {} ({}).",
                p.name,
                crate::engine::fmt_size(p.cache_size)
            ),
            "Package data, global installs and configuration are untouched.".into(),
        ];
        self.confirm = Some(ConfirmDialog {
            title: "Clean package cache?".into(),
            lines,
            danger: false,
            action: PendingAction::RunClean(CleanPlan {
                catalog: vec![],
                orphans: vec![],
                pkg_cache: vec![p.id.to_string()],
                pkg_remove: vec![],
                browser_pause: vec![],
                browser_cache: vec![],
            }),
        });
    }

    pub fn confirm_pkg_remove(&mut self, p: PkgInfo) {
        let mut lines = vec![format!(
            "FULL REMOVAL of {} ({}).",
            p.name,
            crate::engine::fmt_size(p.full_size)
        )];
        if p.removes_tool {
            lines.push("This deletes the tool itself, not just data.".into());
        }
        if !p.note.is_empty() {
            lines.push(p.note.to_string());
        }
        for (cp, sz) in p.full_paths.iter().chain(p.cache_paths.iter()).take(6) {
            lines.push(format!(
                "— {} ({})",
                cp.display(),
                crate::engine::fmt_size(*sz)
            ));
        }
        lines.push("Everything under these folders is gone forever.".into());
        self.confirm = Some(ConfirmDialog {
            title: "Remove package data?".into(),
            lines,
            danger: true,
            action: PendingAction::RunClean(CleanPlan {
                catalog: vec![],
                orphans: vec![],
                pkg_cache: vec![],
                pkg_remove: vec![p.id.to_string()],
                browser_pause: vec![],
                browser_cache: vec![],
            }),
        });
    }

    pub fn confirm_pause(&mut self, id: &str) {
        self.confirm = Some(ConfirmDialog {
            title: "Pause browser updates?".into(),
            lines: vec![
                "Disables updater scheduled tasks and services,".into(),
                "and writes official vendor update policies.".into(),
                "Resume later with one click — nothing is deleted.".into(),
            ],
            danger: false,
            action: PendingAction::RunClean(CleanPlan {
                catalog: vec![],
                orphans: vec![],
                pkg_cache: vec![],
                pkg_remove: vec![],
                browser_pause: vec![id.to_string()],
                browser_cache: vec![],
            }),
        });
    }

    pub fn confirm_resume(&mut self, id: &str) {
        self.confirm = Some(ConfirmDialog {
            title: "Resume browser updates?".into(),
            lines: vec![
                "Re-enables updater tasks and services".into(),
                "and removes the update policies.".into(),
            ],
            danger: false,
            action: PendingAction::ResumeBrowsers(vec![id.to_string()]),
        });
    }

    pub fn confirm_pause_all(&mut self, browsers: &[BrowserInfo]) {
        let ids: Vec<String> = browsers.iter().map(|b| b.id.to_string()).collect();
        self.confirm = Some(ConfirmDialog {
            title: "Pause updates for ALL browsers?".into(),
            lines: vec![
                format!("{} browser(s) will stop auto-updating.", ids.len()),
                "Resume any time from this page.".into(),
            ],
            danger: false,
            action: PendingAction::RunClean(CleanPlan {
                catalog: vec![],
                orphans: vec![],
                pkg_cache: vec![],
                pkg_remove: vec![],
                browser_pause: ids,
                browser_cache: vec![],
            }),
        });
    }

    pub fn confirm_resume_all(&mut self, browsers: &[BrowserInfo]) {
        let ids: Vec<String> = browsers.iter().map(|b| b.id.to_string()).collect();
        self.confirm = Some(ConfirmDialog {
            title: "Resume updates for ALL browsers?".into(),
            lines: vec!["All paused browsers will auto-update again.".into()],
            danger: false,
            action: PendingAction::ResumeBrowsers(ids),
        });
    }

    pub fn confirm_pause_and_clean(&mut self, browsers: &[BrowserInfo]) {
        let ids: Vec<String> = browsers.iter().map(|b| b.id.to_string()).collect();
        self.confirm = Some(ConfirmDialog {
            title: "Pause updates + clean caches?".into(),
            lines: vec![
                format!("Pause updates on {} browser(s).", ids.len()),
                "Then clean their caches (login data untouched).".into(),
            ],
            danger: false,
            action: PendingAction::RunClean(CleanPlan {
                catalog: vec![],
                orphans: vec![],
                pkg_cache: vec![],
                pkg_remove: vec![],
                browser_pause: ids.clone(),
                browser_cache: ids,
            }),
        });
    }

    pub fn confirm_browser_cache(&mut self, id: &str, size: u64) {
        self.confirm = Some(ConfirmDialog {
            title: "Clean browser cache?".into(),
            lines: vec![
                format!(
                    "Deletes {} of cached web data.",
                    crate::engine::fmt_size(size)
                ),
                "Passwords, history, bookmarks and logins are NEVER touched.".into(),
            ],
            danger: false,
            action: PendingAction::RunClean(CleanPlan {
                catalog: vec![],
                orphans: vec![],
                pkg_cache: vec![],
                pkg_remove: vec![],
                browser_pause: vec![],
                browser_cache: vec![id.to_string()],
            }),
        });
    }

    pub fn run_official_uninstaller(&mut self, id: &str) {
        let hints = packages::MANAGERS
            .iter()
            .find(|d| d.id == id)
            .map(|d| d.uninstall_names)
            .unwrap_or(&[]);
        let name = packages::MANAGERS
            .iter()
            .find(|d| d.id == id)
            .map(|d| d.name)
            .unwrap_or("package");
        match packages::find_official_uninstaller(hints) {
            Some((display, us)) => {
                self.confirm = Some(ConfirmDialog {
                    title: "Launch official uninstaller?".into(),
                    lines: vec![
                        format!("Vendor uninstaller: {display}"),
                        "The uninstaller UI will open; complete it manually.".into(),
                    ],
                    danger: false,
                    action: PendingAction::PkgUninstall {
                        id: id.to_string(),
                        display: us,
                    },
                });
            }
            None => {
                log::warn(format!("No official uninstaller registered for {name}."));
            }
        }
    }

    // ------------------------------------------------------------- progress ui

    pub fn scan_running(&self) -> bool {
        self.scan_running
    }

    pub fn clean_running(&self) -> bool {
        self.clean_running
    }

    pub fn clean_progress_frac(&self) -> f32 {
        if self.clean_total == 0 {
            0.0
        } else {
            (self.clean_freed as f32 / self.clean_total as f32).min(1.0)
        }
    }

    fn drain_events(&mut self) {
        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                Event::Log(level, text) => log::log(level, text),
                Event::ScanPhase(p) => self.scan_phase = Some(p),
                Event::ScanDone(res) => {
                    self.scan_running = false;
                    self.scan_phase = None;
                    match *res {
                        Ok(data) => self.scan_data = Some(data),
                        Err(e) => {
                            log::error(format!("Scan failed: {e}"));
                            self.scan_data = None;
                        }
                    }
                }
                Event::CleanProgress(p) => match p {
                    Progress::Phase(s) => self.clean_phase = Some(s),
                    Progress::Item(s) => self.clean_current = Some(s),
                    Progress::Cleaned { freed, item } => {
                        self.clean_freed += freed;
                        log::ok(format!("{item}: {} freed", crate::engine::fmt_size(freed)));
                    }
                    Progress::Failed { item, why } => {
                        log::warn(format!("failed {item}: {why}"));
                    }
                },
                Event::CleanDone(res) => {
                    self.clean_running = false;
                    self.clean_phase = None;
                    self.clean_current = None;
                    match *res {
                        Ok(summary) => {
                            self.summary_open = Some(summary);
                            self.burst_pending = true;
                        }
                        Err(e) => log::error(format!("Clean failed: {e}")),
                    }
                }
            }
        }
    }

    // ------------------------------------------------------------------ frame

    pub fn draw(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        self.drain_events();
        let t = self.theme;

        // auto-scan on first frames
        if self.scan_data.is_none() && !self.scan_running {
            self.spawn_scan();
        }

        // left navigation
        egui::Panel::left("nav")
            .exact_size(224.0)
            .resizable(false)
            .frame(
                egui::Frame::default()
                    .fill(t.panel)
                    .inner_margin(egui::Margin::same(14)),
            )
            .show(ui, |ui| {
                self.draw_logo(ui);
                ui.add_space(18.0);
                for page in Page::all() {
                    self.draw_nav_item(ui, page);
                }
                ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
                    ui.add_space(12.0);
                    if self.scan_running {
                        ui.label(
                            egui::RichText::new(
                                self.scan_phase.as_deref().unwrap_or("Scanning..."),
                            )
                            .size(9.5)
                            .color(t.faint),
                        );
                    }
                    ui.horizontal(|ui| {
                        widgets::pulse_dot(ui, if self.clean_running { t.warn } else { t.ok }, &t);
                        let elevated = crate::platform::is_elevated();
                        widgets::chip(
                            ui,
                            if elevated { "ADMIN" } else { "NO ADMIN" },
                            if elevated { t.ok } else { t.danger },
                            &t,
                        );
                        ui.label(egui::RichText::new("v1.0").size(10.0).color(t.faint));
                    });
                });
            });

        // main content
        egui::CentralPanel::default()
            .frame(
                egui::Frame::default()
                    .fill(t.bg)
                    .inner_margin(egui::Margin::same(20)),
            )
            .show(ui, |ui| {
                let fade_gen = self.fade_gen;
                egui::ScrollArea::vertical()
                    .auto_shrink(false)
                    .show(ui, |ui| {
                        match self.page {
                            Page::Dashboard => dashboard::show(self, ui, &t),
                            Page::DeepClean => deep_clean::show(self, ui, &t),
                            Page::Packages => pk_page::show(self, ui, &t),
                            Page::Browsers => br_page::show(self, ui, &t),
                            Page::Settings => set_page::show(self, ui, &t),
                            Page::Log => log_view::show(self, ui, &t),
                        }
                        let vis = ui.clip_rect();
                        let fade = widgets::anim(ui, &format!("page-fade-{fade_gen}"), 1.0, 0.35);
                        widgets::fade_cover(ui, vis, &t, fade);
                    });
            });

        // running overlay (clean progress)
        if self.clean_running {
            self.draw_clean_overlay(ctx, &t);
        }

        // confirmation modal
        if self.confirm.is_some() {
            self.draw_confirm_modal(ctx, &t);
        }

        // summary modal
        if self.summary_open.is_some() {
            self.draw_summary_modal(ctx, &t);
        }

        // package details modal
        if self.pkg_details_open.is_some() {
            self.draw_pkg_details(ctx, &t);
        }

        // bursts
        let now = ctx.input(|i| i.time);
        if self.burst_pending {
            let c = ctx.viewport_rect().center();
            self.bursts.push(Burst::new(c, now));
            self.burst_pending = false;
        }
        if !self.bursts.is_empty() {
            let screen = ctx.viewport_rect();
            egui::Area::new(Id::new("bursts"))
                .order(egui::Order::Tooltip)
                .fixed_pos(screen.min)
                .interactable(false)
                .show(ctx, |ui| {
                    ui.set_width(screen.width());
                    ui.set_height(screen.height());
                    let painter = ui.painter().clone();
                    let bursts: Vec<&Burst> = self.bursts.iter().collect();
                    for b in bursts {
                        b.draw(&painter, &t, now);
                    }
                });
            self.bursts.retain(|b| !b.done(now));
        }

        // keep animating while jobs run
        if self.scan_running || self.clean_running || !self.bursts.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }

    fn draw_logo(&self, ui: &mut egui::Ui) {
        let t = &self.theme;
        let title = "PURGE";
        ui.horizontal(|ui| {
            for (i, ch) in title.chars().enumerate() {
                ui.label(
                    egui::RichText::new(ch)
                        .size(26.0)
                        .strong()
                        .color(t.grad(i as f32 / (title.len() as f32 - 1.0))),
                );
            }
        });
        ui.label(
            egui::RichText::new("neon-grade windows cleaner")
                .size(10.0)
                .color(t.faint),
        );
    }

    fn draw_nav_item(&mut self, ui: &mut egui::Ui, page: Page) {
        let t = self.theme;
        let selected = self.page == page;
        let (rect, resp) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 34.0), Sense::click());
        if resp.clicked() && self.page != page {
            self.page = page;
            self.fade_gen += 1;
        }
        let sel_p = widgets::anim_bool(ui, &format!("navsel-{:?}", page), selected, 0.14);
        let hover_p = widgets::anim_bool(ui, &format!("navhov-{:?}", page), resp.hovered(), 0.1);
        if sel_p > 0.01 {
            ui.painter()
                .rect_filled(rect, 9.0, t.alpha(t.accent, (26.0 * sel_p) as u8));
        }
        if hover_p > 0.01 && !selected {
            ui.painter()
                .rect_filled(rect, 9.0, t.alpha(t.card_hover, (160.0 * hover_p) as u8));
        }
        if sel_p > 0.01 {
            ui.painter().rect_filled(
                Rect::from_min_size(
                    rect.min + Vec2::new(0.0, 7.0),
                    Vec2::new(3.0, rect.height() - 14.0),
                ),
                2.0,
                t.alpha(t.accent, (255.0 * sel_p) as u8),
            );
        }
        ui.painter().text(
            Pos2::new(rect.min.x + 16.0, rect.center().y),
            Align2::LEFT_CENTER,
            page.label(),
            egui::FontId::proportional(13.5),
            if selected { t.text } else { t.dim },
        );
    }

    fn draw_clean_overlay(&mut self, ctx: &egui::Context, t: &Theme) {
        egui::Area::new(Id::new("clean-overlay"))
            .anchor(Align2::RIGHT_BOTTOM, Vec2::new(-20.0, -20.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::default()
                    .fill(t.panel)
                    .stroke(egui::Stroke::new(1.0, t.border))
                    .corner_radius(12.0)
                    .inner_margin(egui::Margin::same(14))
                    .show(ui, |ui| {
                        ui.set_min_width(380.0);
                        ui.label(
                            egui::RichText::new(
                                self.clean_phase.as_deref().unwrap_or("Working..."),
                            )
                            .size(12.5)
                            .strong()
                            .color(t.text),
                        );
                        ui.add_space(4.0);
                        widgets::shimmer_bar(ui, self.clean_progress_frac(), t);
                        if let Some(cur) = &self.clean_current {
                            ui.label(
                                egui::RichText::new(cur.to_string())
                                    .size(10.0)
                                    .color(t.faint),
                            );
                        }
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(format!(
                                    "{} freed",
                                    crate::engine::fmt_size(self.clean_freed)
                                ))
                                .size(11.5)
                                .color(t.ok)
                                .strong(),
                            );
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if widgets::ghost_button(ui, "cancel-clean", "Cancel", t).clicked()
                                {
                                    self.cancel_clean.store(true, Ordering::Relaxed);
                                    log::warn("Cancel requested — finishing current item.");
                                }
                            });
                        });
                    });
            });
    }

    fn draw_confirm_modal(&mut self, ctx: &egui::Context, t: &Theme) {
        let backdrop = ctx.viewport_rect();
        egui::Area::new(Id::new("confirm-backdrop"))
            .order(egui::Order::Foreground)
            .fixed_pos(backdrop.min)
            .show(ctx, |ui| {
                ui.set_width(backdrop.width());
                ui.set_height(backdrop.height());
                ui.painter().rect_filled(
                    backdrop,
                    0.0,
                    Color32::from_rgba_unmultiplied(0, 0, 0, 160),
                );
            });
        self.render_confirm_inner(ctx, t);
    }

    fn render_confirm_inner(&mut self, ctx: &egui::Context, t: &Theme) {
        // centered dialog
        let title = self
            .confirm
            .as_ref()
            .map(|c| c.title.clone())
            .unwrap_or_default();
        let lines = self
            .confirm
            .as_ref()
            .map(|c| c.lines.clone())
            .unwrap_or_default();
        let danger = self.confirm.as_ref().map(|c| c.danger).unwrap_or(false);
        egui::Area::new(Id::new("confirm-dialog"))
            .anchor(Align2::CENTER_CENTER, Vec2::new(0.0, 0.0))
            .order(egui::Order::Tooltip)
            .show(ctx, |ui| {
                egui::Frame::default()
                    .fill(t.panel)
                    .stroke(egui::Stroke::new(
                        1.2,
                        if danger { t.danger } else { t.border_hi },
                    ))
                    .corner_radius(14.0)
                    .inner_margin(egui::Margin::same(20))
                    .show(ui, |ui| {
                        ui.set_min_width(500.0);
                        ui.label(
                            egui::RichText::new(&title)
                                .size(17.0)
                                .strong()
                                .color(if danger { t.danger } else { t.text }),
                        );
                        ui.add_space(8.0);
                        for l in &lines {
                            ui.label(egui::RichText::new(l).size(11.5).color(t.dim));
                        }
                        ui.add_space(14.0);
                        ui.horizontal(|ui| {
                            if widgets::grad_button(
                                ui,
                                "confirm-ok",
                                if danger { "Confirm delete" } else { "Confirm" },
                                t,
                                danger,
                            )
                            .clicked()
                            {
                                let action = self.confirm.take().unwrap().action;
                                self.execute(action);
                            }
                            ui.add_space(8.0);
                            if widgets::ghost_button(ui, "confirm-cancel", "Cancel", t).clicked() {
                                self.confirm = None;
                            }
                        });
                    });
            });
    }

    fn draw_summary_modal(&mut self, ctx: &egui::Context, t: &Theme) {
        let summary = self.summary_open.as_ref().unwrap().clone();
        egui::Area::new(Id::new("summary-dialog"))
            .anchor(Align2::CENTER_CENTER, Vec2::new(0.0, 0.0))
            .order(egui::Order::Tooltip)
            .show(ctx, |ui| {
                egui::Frame::default()
                    .fill(t.panel)
                    .stroke(egui::Stroke::new(1.2, t.ok))
                    .corner_radius(14.0)
                    .inner_margin(egui::Margin::same(20))
                    .show(ui, |ui| {
                        ui.set_min_width(460.0);
                        ui.label(
                            egui::RichText::new("Clean complete")
                                .size(18.0)
                                .strong()
                                .color(t.ok),
                        );
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new(crate::engine::fmt_size(summary.freed))
                                .size(34.0)
                                .strong()
                                .color(t.text),
                        );
                        ui.label(
                            egui::RichText::new("freed from your disk")
                                .size(11.5)
                                .color(t.dim),
                        );
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new(format!(
                                "{} items deleted in {:.1}s — {} locked/skipped",
                                summary.items_deleted,
                                summary.duration_secs,
                                summary.failures.len()
                            ))
                            .size(11.5)
                            .color(t.dim),
                        );
                        match summary.restore_point_ok {
                            Some(true) => ui.label(
                                egui::RichText::new("Restore point created before cleaning.")
                                    .size(10.5)
                                    .color(t.faint),
                            ),
                            Some(false) => ui.label(
                                egui::RichText::new("No restore point was created (System Protection off or 24h limit).")
                                    .size(10.5)
                                    .color(t.warn),
                            ),
                            None => ui.label(
                                egui::RichText::new("Restore points disabled in settings.")
                                    .size(10.5)
                                    .color(t.faint),
                            ),
                        };
                        ui.add_space(14.0);
                        ui.horizontal(|ui| {
                            if widgets::grad_button(ui, "sum-ok", "Nice", t, false).clicked() {
                                self.summary_open = None;
                                self.page = Page::Dashboard;
                            }
                        });
                    });
            });
    }

    fn draw_pkg_details(&mut self, ctx: &egui::Context, t: &Theme) {
        let Some(data) = self.scan_data.as_ref() else {
            return;
        };
        let id = self.pkg_details_open.clone().unwrap();
        let Some(info) = data.packages.iter().find(|p| p.id == id) else {
            self.pkg_details_open = None;
            return;
        };
        egui::Area::new(Id::new("pkg-details"))
            .anchor(Align2::CENTER_CENTER, Vec2::new(0.0, 0.0))
            .order(egui::Order::Tooltip)
            .show(ctx, |ui| {
                egui::Frame::default()
                    .fill(t.panel)
                    .stroke(egui::Stroke::new(1.0, t.border_hi))
                    .corner_radius(14.0)
                    .inner_margin(egui::Margin::same(20))
                    .show(ui, |ui| {
                        ui.set_min_width(560.0);
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(info.name)
                                    .size(16.0)
                                    .strong()
                                    .color(t.text),
                            );
                            widgets::chip(ui, info.category, t.accent, t);
                        });
                        ui.add_space(6.0);
                        ui.label(
                            egui::RichText::new("CACHE LOCATIONS")
                                .size(10.0)
                                .strong()
                                .color(t.dim),
                        );
                        for (cp, sz) in &info.cache_paths {
                            ui.label(
                                egui::RichText::new(format!(
                                    "{} — {}",
                                    cp.display(),
                                    crate::engine::fmt_size(*sz)
                                ))
                                .size(10.5)
                                .color(t.dim),
                            );
                        }
                        ui.add_space(6.0);
                        ui.label(
                            egui::RichText::new("DATA / TOOL LOCATIONS")
                                .size(10.0)
                                .strong()
                                .color(t.dim),
                        );
                        for (cp, sz) in &info.full_paths {
                            ui.label(
                                egui::RichText::new(format!(
                                    "{} — {}",
                                    cp.display(),
                                    crate::engine::fmt_size(*sz)
                                ))
                                .size(10.5)
                                .color(t.dim),
                            );
                        }
                        if !info.note.is_empty() {
                            ui.add_space(6.0);
                            ui.label(
                                egui::RichText::new(info.note)
                                    .size(10.5)
                                    .color(t.warn)
                                    .italics(),
                            );
                        }
                        ui.add_space(12.0);
                        if widgets::ghost_button(ui, "pd-close", "Close", t).clicked() {
                            self.pkg_details_open = None;
                        }
                    });
            });
    }

    fn execute(&mut self, action: PendingAction) {
        match action {
            PendingAction::RunClean(plan) => self.spawn_clean(plan),
            PendingAction::ResumeBrowsers(ids) => {
                self.confirm = None;
                self.spawn_resume(ids);
            }
            PendingAction::PkgUninstall { id, display } => {
                log::action(format!("Launching official uninstaller for {id}..."));
                match packages::launch_uninstaller(&display) {
                    Ok(()) => log::ok(format!("Uninstaller launched: {display}")),
                    Err(e) => log::error(format!("Uninstaller launch failed: {e}")),
                }
            }
        }
    }
}

impl eframe::App for PurgeApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.draw(&ctx, ui);
    }
}
