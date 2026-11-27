//! Dark-neon theme: palette, egui style application, shared color helpers.

use egui::{Color32, Context, CornerRadius, Vec2, Visuals};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Accent {
    Violet,
    Cyan,
    Mint,
    Pink,
}

impl Accent {
    pub fn label(&self) -> &'static str {
        match self {
            Accent::Violet => "Violet",
            Accent::Cyan => "Cyan",
            Accent::Mint => "Mint",
            Accent::Pink => "Pink",
        }
    }

    pub fn all() -> [Accent; 4] {
        [Accent::Violet, Accent::Cyan, Accent::Mint, Accent::Pink]
    }
}

#[derive(Clone, Copy)]
pub struct Theme {
    pub bg: Color32,
    pub bg_deep: Color32,
    pub panel: Color32,
    pub card: Color32,
    pub card_hover: Color32,
    pub border: Color32,
    pub border_hi: Color32,
    pub text: Color32,
    pub dim: Color32,
    pub faint: Color32,
    pub accent: Color32,
    pub accent2: Color32,
    pub ok: Color32,
    pub warn: Color32,
    pub danger: Color32,
}

pub fn theme(accent: Accent) -> Theme {
    let (a1, a2) = match accent {
        Accent::Violet => (
            Color32::from_rgb(124, 92, 255),
            Color32::from_rgb(34, 211, 238),
        ),
        Accent::Cyan => (
            Color32::from_rgb(34, 211, 238),
            Color32::from_rgb(124, 92, 255),
        ),
        Accent::Mint => (
            Color32::from_rgb(52, 211, 153),
            Color32::from_rgb(34, 211, 238),
        ),
        Accent::Pink => (
            Color32::from_rgb(244, 114, 182),
            Color32::from_rgb(124, 92, 255),
        ),
    };
    Theme {
        bg: Color32::from_rgb(11, 14, 20),
        bg_deep: Color32::from_rgb(7, 9, 13),
        panel: Color32::from_rgb(15, 19, 28),
        card: Color32::from_rgb(21, 26, 38),
        card_hover: Color32::from_rgb(28, 34, 50),
        border: Color32::from_rgb(30, 38, 53),
        border_hi: Color32::from_rgb(56, 66, 92),
        text: Color32::from_rgb(230, 234, 242),
        dim: Color32::from_rgb(138, 147, 166),
        faint: Color32::from_rgb(84, 92, 110),
        accent: a1,
        accent2: a2,
        ok: Color32::from_rgb(52, 211, 153),
        warn: Color32::from_rgb(251, 191, 36),
        danger: Color32::from_rgb(248, 113, 113),
    }
}

impl Theme {
    /// Solid interpolation between two colors (manual, no gamma fuss).
    pub fn lerp(&self, from: Color32, to: Color32, t: f32) -> Color32 {
        let t = t.clamp(0.0, 1.0);
        let m = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
        Color32::from_rgba_unmultiplied(
            m(from.r(), to.r()),
            m(from.g(), to.g()),
            m(from.b(), to.b()),
            m(from.a(), to.a()),
        )
    }

    /// The signature violet -> cyan gradient color at position t in [0,1].
    pub fn grad(&self, t: f32) -> Color32 {
        self.lerp(self.accent, self.accent2, t)
    }

    pub fn alpha(&self, c: Color32, a: u8) -> Color32 {
        Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
    }
}

pub fn apply(ctx: &Context, accent: Accent) -> Theme {
    let t = theme(accent);
    let mut style = (*ctx.style_of(egui::Theme::Dark)).clone();
    style.visuals = visuals(&t);
    style.spacing.item_spacing = Vec2::new(10.0, 8.0);
    style.spacing.button_padding = Vec2::new(14.0, 7.0);
    style.spacing.menu_margin = egui::Margin::same(8);
    ctx.set_theme(egui::Theme::Dark);
    ctx.set_style_of(egui::Theme::Dark, style);
    t
}

fn visuals(t: &Theme) -> Visuals {
    let mut v = Visuals::dark();
    v.override_text_color = Some(t.text);
    v.window_fill = t.panel;
    v.panel_fill = t.panel;
    v.extreme_bg_color = t.bg_deep;
    v.faint_bg_color = t.alpha(t.accent, 14);
    v.window_stroke = egui::Stroke::new(1.0, t.border);
    v.widgets.noninteractive.bg_fill = t.card;
    v.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, t.text);
    v.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, t.border);
    v.widgets.inactive.bg_fill = t.card;
    v.widgets.inactive.weak_bg_fill = t.card;
    v.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, t.dim);
    v.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, t.border);
    v.widgets.hovered.bg_fill = t.card_hover;
    v.widgets.hovered.weak_bg_fill = t.card_hover;
    v.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, t.text);
    v.widgets.hovered.bg_stroke = egui::Stroke::new(1.2, t.border_hi);
    v.widgets.active.bg_fill = t.alpha(t.accent, 60);
    v.widgets.active.weak_bg_fill = t.alpha(t.accent, 60);
    v.widgets.active.fg_stroke = egui::Stroke::new(1.0, t.text);
    v.selection.bg_fill = t.alpha(t.accent, 70);
    v.window_corner_radius = CornerRadius::same(10);
    v.menu_corner_radius = CornerRadius::same(10);
    v.widgets.noninteractive.corner_radius = CornerRadius::same(4);
    v.widgets.inactive.corner_radius = CornerRadius::same(8);
    v.widgets.hovered.corner_radius = CornerRadius::same(8);
    v.widgets.active.corner_radius = CornerRadius::same(8);
    v.widgets.open.corner_radius = CornerRadius::same(8);
    v
}
