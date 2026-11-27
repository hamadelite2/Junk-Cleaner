//! Animated neon UI primitives: toggles, cards, rings, shimmer bars, chips,
//! gradient buttons, pulses and completion bursts. All drawn with egui painters.

use egui::{Color32, FontId, Id, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};

use crate::theme::Theme;

const UI_FONT: f32 = 13.5;

// ------------------------------------------------------------------- helpers

pub fn time(ui: &Ui) -> f64 {
    ui.input(|i| i.time)
}

/// Animated float towards a target value.
pub fn anim(ui: &mut Ui, id: &str, target: f32, speed: f32) -> f32 {
    ui.ctx().animate_value_with_time(Id::new(id), target, speed)
}

/// Animated 0..1 bool.
pub fn anim_bool(ui: &mut Ui, id: &str, on: bool, time_s: f32) -> f32 {
    ui.ctx().animate_bool_with_time(Id::new(id), on, time_s)
}

// ------------------------------------------------------------------- toggle

/// iOS-style neon toggle.
pub fn toggle(ui: &mut Ui, id_salt: &str, on: &mut bool, t: &Theme) -> egui::Response {
    let size = Vec2::new(40.0, 20.0);
    let (rect, mut resp) = ui.allocate_exact_size(size, Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let p = anim_bool(ui, &format!("tgl-{id_salt}"), *on, 0.16);
    let track = t.lerp(t.border, t.accent, p);
    let track_hi = t.lerp(t.border_hi, t.grad(0.6), p);
    let r = rect.shrink2(Vec2::new(0.0, 0.0));
    ui.painter().rect_filled(r, r.height() / 2.0, track);
    // glow when on
    if p > 0.05 {
        ui.painter().rect_filled(
            r.expand(2.5 * p),
            r.height() / 2.0,
            t.alpha(track_hi, (46.0 * p) as u8),
        );
    }
    let knob_r = r.height() / 2.0 - 3.0;
    let a = rect.left() + knob_r + 3.0;
    let b = rect.right() - knob_r - 3.0;
    let x = a + (b - a) * p;
    let cy = rect.center().y;
    ui.painter()
        .circle_filled(Pos2::new(x, cy), knob_r, Color32::WHITE);
    resp
}

// ------------------------------------------------------------------ checkbox

/// Neon checkbox with animated fill.
pub fn checkbox(ui: &mut Ui, id_salt: &str, checked: &mut bool, t: &Theme) -> egui::Response {
    let size = Vec2::new(16.0, 16.0);
    let (rect, mut resp) = ui.allocate_exact_size(size, Sense::click());
    if resp.clicked() {
        *checked = !*checked;
        resp.mark_changed();
    }
    let p = anim_bool(ui, &format!("cb-{id_salt}"), *checked, 0.14);
    ui.painter().rect_filled(rect, 4.0, t.alpha(t.card, 255));
    ui.painter().rect_stroke(
        rect,
        4.0,
        Stroke::new(1.2, if p > 0.5 { t.accent } else { t.border_hi }),
        StrokeKind::Inside,
    );
    if p > 0.02 {
        let inner = rect.shrink(3.0);
        ui.painter()
            .rect_filled(inner, 2.5, t.alpha(t.grad(0.5), (255.0 * p) as u8));
    }
    resp
}

// ---------------------------------------------------------------------- card

#[allow(dead_code)]
pub struct CardResponse {
    pub hovered: bool,
    pub clicked: bool,
}

/// Glassy card with hover glow. Content closure renders inside.
pub fn card<R>(
    ui: &mut Ui,
    id_salt: &str,
    t: &Theme,
    accent_tint: bool,
    add: impl FnOnce(&mut Ui) -> R,
) -> CardResponse {
    let out = egui::Frame::default()
        .fill(t.card)
        .stroke(Stroke::new(1.0, t.border))
        .corner_radius(12.0)
        .inner_margin(egui::Margin::same(14))
        .show(ui, |ui| add(ui));
    let hovered = out.response.hovered();
    let clicked = out.response.clicked();
    let hp = anim_bool(ui, &format!("{id_salt}-hover"), hovered, 0.12);
    if hp > 0.01 {
        let border = if accent_tint {
            t.lerp(t.border, t.accent, hp)
        } else {
            t.lerp(t.border, t.border_hi, hp)
        };
        ui.painter().rect_filled(
            out.response.rect,
            12.0,
            t.alpha(t.card_hover, (140.0 * hp) as u8),
        );
        ui.painter().rect_stroke(
            out.response.rect,
            12.0,
            Stroke::new(1.2, t.alpha(border, (200.0 * hp) as u8)),
            StrokeKind::Middle,
        );
    }
    CardResponse { hovered, clicked }
}

// ----------------------------------------------------------------- stat card

/// Stat card: label, big animated value, sub-line.
pub fn stat_card(ui: &mut Ui, id_salt: &str, label: &str, value: &str, sub: &str, t: &Theme) {
    card(ui, id_salt, t, true, |ui| {
        ui.vertical(|ui| {
            ui.label(
                egui::RichText::new(label.to_uppercase())
                    .size(10.5)
                    .color(t.dim)
                    .strong(),
            );
            ui.add_space(2.0);
            ui.label(egui::RichText::new(value).size(22.0).color(t.text).strong());
            if !sub.is_empty() {
                ui.label(egui::RichText::new(sub).size(11.0).color(t.faint));
            }
        });
    });
}

// ----------------------------------------------------------------- neon ring

/// Big circular progress ring with gradient arc + glow + center text.
pub fn neon_ring(ui: &mut Ui, size: f32, progress: f32, title: &str, sub: &str, t: &Theme) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(size, size), Sense::hover());
    let c = rect.center();
    let radius = size / 2.0 - 10.0;
    let width = 9.0;

    // animated progress
    let p = anim(ui, "ring-progress", progress.clamp(0.0, 1.0), 0.35);

    // pulse glow
    let now = time(ui);
    let pulse = ((now * 1.4).sin() * 0.5 + 0.5) as f32;

    // background ring
    ui.painter()
        .circle_stroke(c, radius, Stroke::new(width, t.alpha(t.border, 255)));

    // glow under-arc
    if p > 0.01 {
        ui.painter().circle_stroke(
            c,
            radius,
            Stroke::new(
                width + 6.0,
                t.alpha(t.grad(0.5), (26.0 + 18.0 * pulse) as u8),
            ),
        );
    }

    // gradient progress arc drawn as many segments
    let segs = 96;
    let start = -std::f32::consts::FRAC_PI_2;
    let sweep = std::f32::consts::TAU * p;
    for i in 0..segs {
        let f0 = i as f32 / segs as f32;
        let f1 = (i + 1) as f32 / segs as f32;
        if f0 > p {
            break;
        }
        let a0 = start + sweep * f0;
        let a1 = start + sweep * f1;
        let col = t.grad(f0);
        let p0 = Pos2::new(c.x + radius * a0.cos(), c.y + radius * a0.sin());
        let p1 = Pos2::new(c.x + radius * a1.cos(), c.y + radius * a1.sin());
        ui.painter().line_segment([p0, p1], Stroke::new(width, col));
    }

    // center text
    let galley_t = ui.painter().layout_no_wrap(
        title.to_string(),
        FontId::proportional(size * 0.135),
        t.text,
    );
    let galley_s =
        ui.painter()
            .layout_no_wrap(sub.to_string(), FontId::proportional(size * 0.07), t.dim);
    let (tw, th) = (galley_t.size().x, galley_t.size().y);
    let cy = c.y - th / 2.0 - 3.0;
    ui.painter()
        .galley(Pos2::new(c.x - tw / 2.0, cy), galley_t, t.text);
    ui.painter().galley(
        Pos2::new(c.x - galley_s.size().x / 2.0, cy + th + 4.0),
        galley_s,
        t.dim,
    );
}

// --------------------------------------------------------------- shimmer bar

/// Horizontal progress bar with animated gradient + moving highlight.
pub fn shimmer_bar(ui: &mut Ui, frac: f32, t: &Theme) {
    let avail = ui.available_size_before_wrap();
    let height = 10.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(avail.x, height), Sense::hover());
    let p = anim(ui, "shimmer-frac", frac.clamp(0.0, 1.0), 0.3);

    ui.painter().rect_filled(rect, height / 2.0, t.bg_deep);

    if p > 0.001 {
        let fill = Rect::from_min_size(rect.min, Vec2::new(rect.width() * p, height));
        // gradient fill via segments
        let segs = 48;
        for i in 0..segs {
            let f = i as f32 / segs as f32;
            let w = fill.width() / segs as f32;
            let seg = Rect::from_min_size(
                Pos2::new(fill.min.x + w * i as f32, fill.min.y),
                Vec2::new(w + 0.5, height),
            );
            ui.painter().rect_filled(seg, height / 2.0, t.grad(f));
        }
        // moving highlight
        let now = time(ui) as f32;
        let hl_x = (now * 0.35).fract();
        let hl_w = rect.width() * 0.18;
        let hl = Rect::from_min_size(
            Pos2::new(
                fill.min.x + (rect.width() * hl_x).min(fill.width() - hl_w),
                fill.min.y,
            ),
            Vec2::new(hl_w, height),
        );
        if hl.width() > 0.0 {
            ui.painter().rect_filled(
                hl,
                height / 2.0,
                Color32::from_rgba_unmultiplied(255, 255, 255, 34),
            );
        }
    }
}

// --------------------------------------------------------------------- chips

pub fn chip(ui: &mut Ui, text: &str, color: Color32, t: &Theme) {
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_string(), FontId::proportional(10.5), color);
    let pad = Vec2::new(9.0, 5.0);
    let size = galley.size() + pad * 2.0;
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter().rect_filled(rect, 9.0, t.alpha(color, 26));
    ui.painter().rect_stroke(
        rect,
        9.0,
        Stroke::new(1.0, t.alpha(color, 120)),
        StrokeKind::Inside,
    );
    ui.painter().galley(
        Pos2::new(rect.min.x + pad.x, rect.center().y - galley.size().y / 2.0),
        galley,
        color,
    );
}

/// Small pulsing status dot.
pub fn pulse_dot(ui: &mut Ui, color: Color32, t: &Theme) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(10.0, 10.0), Sense::hover());
    let now = time(ui) as f32;
    let pulse = (now * 1.8).sin() * 0.5 + 0.5;
    ui.painter().circle_filled(
        rect.center(),
        3.4,
        t.alpha(color, (90.0 + 110.0 * pulse) as u8),
    );
    let _ = t;
}

// ------------------------------------------------------------ section header

pub fn section_header(ui: &mut Ui, title: &str, subtitle: &str, t: &Theme) {
    ui.vertical(|ui| {
        ui.label(egui::RichText::new(title).size(19.0).strong().color(t.text));
        if !subtitle.is_empty() {
            ui.label(egui::RichText::new(subtitle).size(11.5).color(t.dim));
        }
        ui.add_space(2.0);
        let w = ui.available_width().min(340.0);
        let rect = ui.allocate_exact_size(Vec2::new(w, 2.5), Sense::hover()).0;
        let segs = 30;
        for i in 0..segs {
            let f = i as f32 / segs as f32;
            let sw = rect.width() / segs as f32;
            let seg = Rect::from_min_size(
                Pos2::new(rect.min.x + sw * i as f32, rect.min.y),
                Vec2::new(sw + 0.5, 2.5),
            );
            ui.painter().rect_filled(seg, 1.0, t.grad(f));
        }
    });
    ui.add_space(6.0);
}

// ------------------------------------------------------------------- buttons

/// Gradient primary button.
pub fn grad_button(
    ui: &mut Ui,
    id_salt: &str,
    text: &str,
    t: &Theme,
    danger: bool,
) -> egui::Response {
    let pad = Vec2::new(18.0, 9.0);
    let galley = ui.painter().layout_no_wrap(
        text.to_string(),
        FontId::proportional(UI_FONT + 1.0),
        Color32::WHITE,
    );
    let size = galley.size() + pad * 2.0;
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let hover_p = anim_bool(ui, &format!("btn-{id_salt}"), resp.hovered(), 0.12);
    let (c1, c2) = if danger {
        (
            Color32::from_rgb(220, 60, 74),
            Color32::from_rgb(248, 113, 113),
        )
    } else {
        (t.accent, t.accent2)
    };
    let segs = 36;
    for i in 0..segs {
        let f = i as f32 / segs as f32;
        let sw = rect.width() / segs as f32;
        let seg = Rect::from_min_size(
            Pos2::new(rect.min.x + sw * i as f32, rect.min.y),
            Vec2::new(sw + 0.5, rect.height()),
        );
        let col = t.lerp(c1, c2, f);
        let col = t.lerp(col, Color32::WHITE, 0.12 * hover_p);
        ui.painter().rect_filled(seg, 9.0, col);
    }
    if hover_p > 0.05 {
        ui.painter().rect_stroke(
            rect.expand(2.5),
            11.0,
            Stroke::new(
                1.2,
                t.alpha(
                    if danger { t.danger } else { t.accent },
                    (70.0 * hover_p) as u8,
                ),
            ),
            StrokeKind::Middle,
        );
    }
    ui.painter().galley(
        Pos2::new(
            rect.center().x - galley.size().x / 2.0,
            rect.center().y - galley.size().y / 2.0,
        ),
        galley,
        Color32::WHITE,
    );
    resp
}

/// Ghost (outline) button.
pub fn ghost_button(ui: &mut Ui, id_salt: &str, text: &str, t: &Theme) -> egui::Response {
    let pad = Vec2::new(16.0, 8.0);
    let galley =
        ui.painter()
            .layout_no_wrap(text.to_string(), FontId::proportional(UI_FONT), t.text);
    let size = galley.size() + pad * 2.0;
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let hover_p = anim_bool(ui, &format!("ghost-{id_salt}"), resp.hovered(), 0.12);
    ui.painter()
        .rect_filled(rect, 9.0, t.alpha(t.card_hover, (255.0 * hover_p) as u8));
    ui.painter().rect_stroke(
        rect,
        9.0,
        Stroke::new(1.0, t.lerp(t.border, t.border_hi, hover_p)),
        StrokeKind::Inside,
    );
    ui.painter().galley(
        Pos2::new(
            rect.center().x - galley.size().x / 2.0,
            rect.center().y - galley.size().y / 2.0,
        ),
        galley,
        t.text,
    );
    resp
}

// ------------------------------------------------------------------- bursts

/// Simple radial particle burst for the completion moment.
pub struct Burst {
    pub t0: f64,
    pub center: Pos2,
}

impl Burst {
    pub fn new(center: Pos2, now: f64) -> Burst {
        Burst { t0: now, center }
    }

    pub fn draw(&self, painter: &egui::Painter, t: &Theme, now: f64) {
        let age = (now - self.t0) as f32;
        if age < 0.0 || age > 1.6 {
            return;
        }
        let k = age / 1.6;
        let alpha = ((1.0 - k) * 200.0) as u8;
        let n = 26;
        for i in 0..n {
            let a = (i as f32 / n as f32) * std::f32::consts::TAU;
            let speed = 60.0 + ((i * 37) % 40) as f32;
            let d = speed * k * 1.2;
            let p = Pos2::new(
                self.center.x + a.cos() * d,
                self.center.y + a.sin() * d * 0.7,
            );
            let r = (2.6 * (1.0 - k)).max(0.4);
            painter.circle_filled(p, r, t.alpha(t.grad(i as f32 / n as f32), alpha));
        }
    }

    pub fn done(&self, now: f64) -> bool {
        now - self.t0 > 1.6
    }
}

// ------------------------------------------------------------------ misc

/// Fade-in cover: draw AFTER page content with the bg color at (1-alpha).
pub fn fade_cover(ui: &mut Ui, rect: Rect, t: &Theme, fade: f32) {
    let a = (1.0 - fade).clamp(0.0, 1.0);
    if a > 0.01 {
        ui.painter()
            .rect_filled(rect, 0.0, t.alpha(t.bg, (a * 255.0) as u8));
    }
}

/// Monospace-ish path line, dimmed, with ellipsis on the left when too long.
pub fn path_line(ui: &mut Ui, path: &str, t: &Theme) {
    let avail = ui.available_width() - 4.0;
    let mut text = path.to_string();
    let font = FontId::monospace(10.5);
    loop {
        let g = ui
            .painter()
            .layout_no_wrap(text.clone(), font.clone(), t.faint);
        if g.size().x <= avail || text.len() < 8 {
            break;
        }
        // trim from the left
        let cut = (text.len() / 3).max(1);
        text = format!("…{}", &text[cut.min(text.len() - 1)..]);
    }
    ui.label(egui::RichText::new(&text).size(10.5).color(t.faint));
}
