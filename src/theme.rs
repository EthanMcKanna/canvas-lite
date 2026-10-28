//! Design system: a monochrome palette, Geist type, and the handful of drawn
//! widgets everything else is built from. Colour is reserved for meaning:
//! course identity (a small square) and danger (overdue / missing).

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Pos2, Rect, Response, RichText, Sense,
    Stroke, StrokeKind, TextStyle, Theme, Ui, Vec2, pos2, vec2,
};
use std::sync::Arc;

#[derive(Clone, Copy)]
pub struct Pal {
    pub bg: Color32,
    pub sidebar: Color32,
    pub hover: Color32,
    pub raised: Color32,
    pub border: Color32,
    pub border_strong: Color32,
    pub text: Color32,
    pub text2: Color32,
    pub text3: Color32,
    pub text4: Color32,
    pub danger: Color32,
    /// Foreground on an inverted (text-coloured) fill.
    pub on_inverse: Color32,
}

const DARK: Pal = Pal {
    bg: Color32::from_rgb(0, 0, 0),
    sidebar: Color32::from_rgb(7, 7, 7),
    hover: Color32::from_rgb(17, 17, 17),
    raised: Color32::from_rgb(12, 12, 12),
    border: Color32::from_rgb(28, 28, 28),
    border_strong: Color32::from_rgb(46, 46, 46),
    text: Color32::from_rgb(237, 237, 237),
    text2: Color32::from_rgb(161, 161, 161),
    text3: Color32::from_rgb(110, 110, 110),
    text4: Color32::from_rgb(64, 64, 64),
    danger: Color32::from_rgb(255, 99, 99),
    on_inverse: Color32::from_rgb(0, 0, 0),
};

const LIGHT: Pal = Pal {
    bg: Color32::from_rgb(255, 255, 255),
    sidebar: Color32::from_rgb(250, 250, 250),
    hover: Color32::from_rgb(244, 244, 244),
    raised: Color32::from_rgb(250, 250, 250),
    border: Color32::from_rgb(234, 234, 234),
    border_strong: Color32::from_rgb(212, 212, 212),
    text: Color32::from_rgb(10, 10, 10),
    text2: Color32::from_rgb(90, 90, 90),
    text3: Color32::from_rgb(143, 143, 143),
    text4: Color32::from_rgb(200, 200, 200),
    danger: Color32::from_rgb(218, 48, 54),
    on_inverse: Color32::from_rgb(255, 255, 255),
};

pub fn pal(ui: &Ui) -> Pal {
    if ui.visuals().dark_mode { DARK } else { LIGHT }
}

// ---------- type ----------

/// Type scale. Everything uses one of these; mono only for numbers, times and keys.
pub mod size {
    /// Tracked caps, tiny meta.
    pub const XS: f32 = 11.0;
    /// Secondary text, meta, column values.
    pub const SM: f32 = 12.5;
    /// Body, rows, controls.
    pub const BASE: f32 = 13.5;
    /// Emphasised body (announcement titles, syllabus headings).
    pub const MD: f32 = 15.0;
    /// Page hero (course code).
    pub const XL: f32 = 28.0;
}

pub fn medium() -> FontFamily {
    FontFamily::Name("medium".into())
}
pub fn semibold() -> FontFamily {
    FontFamily::Name("semibold".into())
}
pub fn mono_medium() -> FontFamily {
    FontFamily::Name("mono-medium".into())
}

pub fn txt(s: impl Into<String>, size: f32, color: Color32) -> RichText {
    RichText::new(s).size(size).color(color)
}
pub fn med(s: impl Into<String>, size: f32, color: Color32) -> RichText {
    txt(s, size, color).family(medium())
}
pub fn semi(s: impl Into<String>, size: f32, color: Color32) -> RichText {
    txt(s, size, color).family(semibold())
}
pub fn mono(s: impl Into<String>, size: f32, color: Color32) -> RichText {
    txt(s, size, color).family(FontFamily::Monospace)
}

pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let mut add = |name: &str, bytes: &'static [u8]| {
        fonts.font_data.insert(name.into(), Arc::new(FontData::from_static(bytes)));
    };
    add("geist", include_bytes!("../assets/fonts/Geist-Regular.ttf"));
    add("geist-medium", include_bytes!("../assets/fonts/Geist-Medium.ttf"));
    add("geist-semibold", include_bytes!("../assets/fonts/Geist-SemiBold.ttf"));
    add("geist-mono", include_bytes!("../assets/fonts/GeistMono-Regular.ttf"));
    add("geist-mono-medium", include_bytes!("../assets/fonts/GeistMono-Medium.ttf"));
    // Keep egui's bundled fonts as fallbacks for glyphs Geist lacks.
    let fallbacks = fonts.families[&FontFamily::Proportional].clone();
    let family =
        |primary: &str| std::iter::once(primary.to_string()).chain(fallbacks.iter().cloned()).collect::<Vec<_>>();
    fonts.families.insert(FontFamily::Proportional, family("geist"));
    fonts.families.insert(FontFamily::Monospace, family("geist-mono"));
    fonts.families.insert(medium(), family("geist-medium"));
    fonts.families.insert(semibold(), family("geist-semibold"));
    fonts.families.insert(mono_medium(), family("geist-mono-medium"));
    ctx.set_fonts(fonts);

    for (theme, p) in [(Theme::Dark, DARK), (Theme::Light, LIGHT)] {
        ctx.set_visuals_of(theme, visuals(theme == Theme::Dark, p));
    }
    ctx.all_styles_mut(|s| {
        s.text_styles = [
            (TextStyle::Small, FontId::proportional(11.5)),
            (TextStyle::Body, FontId::proportional(13.5)),
            (TextStyle::Button, FontId::proportional(13.0)),
            (TextStyle::Heading, FontId::new(20.0, semibold())),
            (TextStyle::Monospace, FontId::monospace(12.5)),
        ]
        .into();
        s.spacing.item_spacing = vec2(8.0, 6.0);
        s.spacing.button_padding = vec2(10.0, 5.0);
        s.spacing.interact_size.y = 28.0;
        s.spacing.scroll = egui::style::ScrollStyle::floating();
        s.spacing.scroll.floating_width = 6.0;
        s.interaction.selectable_labels = false;
        s.interaction.tooltip_delay = 0.35;
        s.spacing.tooltip_width = 320.0;
        s.spacing.menu_margin = egui::Margin::symmetric(10, 7);
    });
}

fn visuals(dark: bool, p: Pal) -> egui::Visuals {
    let mut v = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    v.override_text_color = Some(p.text);
    v.weak_text_color = Some(p.text3);
    v.panel_fill = p.bg;
    v.window_fill = p.raised;
    v.window_stroke = Stroke::new(1.0, p.border);
    v.extreme_bg_color = p.bg;
    v.text_edit_bg_color = Some(p.bg);
    v.faint_bg_color = p.hover;
    v.hyperlink_color = p.text;
    v.error_fg_color = p.danger;
    v.selection.bg_fill = p.text.gamma_multiply(0.18);
    v.selection.stroke = Stroke::new(1.0, p.text2);
    v.window_corner_radius = CornerRadius::same(8);
    v.menu_corner_radius = CornerRadius::same(8);
    // Tooltips/popups: a raised panel with a hairline and a soft, short shadow.
    v.popup_shadow = egui::epaint::Shadow {
        offset: [0, 4],
        blur: 16,
        spread: 0,
        color: Color32::from_black_alpha(if dark { 140 } else { 28 }),
    };
    v.window_shadow = v.popup_shadow;
    let r = CornerRadius::same(6);
    for (w, fill, stroke, fg) in [
        (&mut v.widgets.noninteractive, p.bg, p.border, p.text2),
        (&mut v.widgets.inactive, p.bg, p.border_strong, p.text),
        (&mut v.widgets.hovered, p.hover, p.border_strong, p.text),
        (&mut v.widgets.active, p.hover, p.text3, p.text),
        (&mut v.widgets.open, p.hover, p.border_strong, p.text),
    ] {
        w.bg_fill = fill;
        w.weak_bg_fill = fill;
        w.bg_stroke = Stroke::new(1.0, stroke);
        w.fg_stroke = Stroke::new(1.0, fg);
        w.corner_radius = r;
        w.expansion = 0.0;
    }
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    v
}

// ---------- primitives ----------

/// Give a painted widget an accessibility node (VoiceOver, UI automation).
pub fn a11y(resp: &Response, role: egui::Role, label: &str, selected: Option<bool>) {
    let label = label.to_string();
    resp.widget_info(move || match selected {
        Some(s) => egui::WidgetInfo::selected(role, true, s, &label),
        None => egui::WidgetInfo::labeled(role, true, &label),
    });
}

pub fn hairline(ui: &mut Ui) {
    let p = pal(ui);
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().hline(r.x_range(), r.center().y, Stroke::new(1.0, p.border));
}

/// Text laid out on one line, truncated with an ellipsis to `max_w`.
pub fn galley(ui: &Ui, text: &str, font: FontId, color: Color32, max_w: f32) -> Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_string(), font, color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(max_w.max(8.0));
    ui.painter().layout_job(job)
}

pub fn course_square(ui: &Ui, center: Pos2, color: Color32, size: f32) {
    ui.painter().rect_filled(Rect::from_center_size(center, Vec2::splat(size)), 1.5, color);
}

#[derive(Clone, Copy, PartialEq)]
pub enum Status {
    Todo,
    Done,
    Missing,
    /// Not a to-do: an exam marker (diamond). Never clickable.
    Exam {
        past: bool,
    },
}

/// Linear-style status circle: hollow for to-do, filled with a check when done,
/// red outline when missing. Returns the click response.
pub fn status_circle(ui: &mut Ui, center: Pos2, status: Status, enabled: bool, id: egui::Id) -> Response {
    let p = pal(ui);
    let rect = Rect::from_center_size(center, Vec2::splat(22.0));
    let resp = ui.interact(rect, id, if enabled { Sense::click() } else { Sense::hover() });
    if enabled {
        a11y(&resp, egui::Role::CheckBox, "Mark complete", Some(status == Status::Done));
    }
    let hot = enabled && resp.hovered();
    let painter = ui.painter();
    let r = 6.5;
    match status {
        Status::Done => {
            painter.circle_filled(center, r, if hot { p.text } else { p.text2 });
            let s = Stroke::new(1.5, p.on_inverse);
            painter.line_segment([center + vec2(-2.8, 0.1), center + vec2(-0.7, 2.2)], s);
            painter.line_segment([center + vec2(-0.7, 2.2), center + vec2(3.0, -2.0)], s);
        }
        Status::Todo => {
            painter.circle_stroke(center, r, Stroke::new(1.3, if hot { p.text } else { p.text3 }));
        }
        Status::Missing => {
            painter.circle_stroke(center, r, Stroke::new(1.3, p.danger));
            if hot {
                painter.circle_filled(center, 2.0, p.danger);
            }
        }
        Status::Exam { past } => {
            let d = 6.5;
            let pts =
                vec![center + vec2(0.0, -d), center + vec2(d, 0.0), center + vec2(0.0, d), center + vec2(-d, 0.0)];
            let c = if past { p.text3 } else { p.text };
            painter.add(egui::Shape::closed_line(pts, Stroke::new(1.3, c)));
            if !past {
                painter.circle_filled(center, 1.6, c);
            }
        }
    }
    if enabled { resp.on_hover_cursor(egui::CursorIcon::PointingHand) } else { resp }
}

#[derive(Clone, Copy)]
pub enum Icon {
    ChevronLeft,
    ChevronRight,
    ChevronDown,
    List,
    Calendar,
    Settings,
    External,
}

pub fn paint_icon(ui: &Ui, rect: Rect, icon: Icon, color: Color32) {
    let painter = ui.painter();
    let s = Stroke::new(1.3, color);
    let c = rect.center();
    let line = |a: Vec2, b: Vec2| {
        painter.line_segment([c + a, c + b], s);
    };
    match icon {
        Icon::ChevronLeft => {
            line(vec2(2.0, -4.0), vec2(-2.0, 0.0));
            line(vec2(-2.0, 0.0), vec2(2.0, 4.0));
        }
        Icon::ChevronRight => {
            line(vec2(-2.0, -4.0), vec2(2.0, 0.0));
            line(vec2(2.0, 0.0), vec2(-2.0, 4.0));
        }
        Icon::ChevronDown => {
            line(vec2(-4.0, -2.0), vec2(0.0, 2.0));
            line(vec2(0.0, 2.0), vec2(4.0, -2.0));
        }
        Icon::List => {
            for y in [-4.0, 0.0, 4.0] {
                painter.circle_filled(c + vec2(-5.0, y), 0.9, color);
                line(vec2(-2.5, y), vec2(6.0, y));
            }
        }
        Icon::Calendar => {
            let r = Rect::from_center_size(c + vec2(0.0, 0.5), vec2(12.0, 11.0));
            painter.rect_stroke(r, 2.0, s, StrokeKind::Middle);
            painter.hline(r.x_range(), r.top() + 3.5, s);
            line(vec2(-3.0, -7.0), vec2(-3.0, -4.0));
            line(vec2(3.0, -7.0), vec2(3.0, -4.0));
        }
        Icon::Settings => {
            // Gear: eight flat-topped teeth around a hub.
            let teeth = 8;
            let (outer, inner) = (7.0_f32, 5.2_f32);
            let mut pts = Vec::with_capacity(teeth * 4);
            for i in 0..teeth {
                let a = i as f32 / teeth as f32 * std::f32::consts::TAU;
                let step = std::f32::consts::TAU / teeth as f32;
                for (da, r) in [(-0.30, inner), (-0.17, outer), (0.17, outer), (0.30, inner)] {
                    let t = a + da * step * 1.6;
                    pts.push(c + vec2(t.cos(), t.sin()) * r);
                }
            }
            painter.add(egui::Shape::closed_line(pts, s));
            painter.circle_stroke(c, 2.2, s);
        }
        Icon::External => {
            line(vec2(-3.5, 3.5), vec2(3.5, -3.5));
            line(vec2(-1.0, -3.5), vec2(3.5, -3.5));
            line(vec2(3.5, -3.5), vec2(3.5, 1.0));
        }
    }
}

/// Text with extra letter spacing — used for small uppercase labels.
pub fn tracked(ui: &Ui, text: &str, font: FontId, color: Color32, spacing: f32) -> Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::default();
    job.append(
        text,
        0.0,
        egui::TextFormat { font_id: font, color, extra_letter_spacing: spacing, ..Default::default() },
    );
    ui.painter().layout_job(job)
}

/// A keycap: mono glyph in a hairline box.
pub fn kbd(ui: &mut Ui, key: &str) {
    let p = pal(ui);
    let g = ui.painter().layout_no_wrap(key.to_string(), FontId::monospace(11.5), p.text2);
    let size = vec2((g.size().x + 10.0).max(20.0), 20.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter().rect(rect, 4.0, p.raised, Stroke::new(1.0, p.border_strong), StrokeKind::Inside);
    ui.painter().galley(rect.center() - g.size() / 2.0, g, p.text2);
}

/// Square, borderless icon button that lights up on hover.
pub fn icon_button(ui: &mut Ui, icon: Icon, tip: &str) -> Response {
    let p = pal(ui);
    let (rect, resp) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect, 6.0, p.hover);
    }
    paint_icon(ui, rect, icon, if resp.hovered() { p.text } else { p.text2 });
    a11y(&resp, egui::Role::Button, tip, None);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(tip)
}

/// Bordered text button. `primary` inverts it (white on black / black on white).
pub fn button(ui: &mut Ui, label: &str, primary: bool) -> Response {
    let p = pal(ui);
    let font = FontId::new(13.0, medium());
    let g = ui.painter().layout_no_wrap(label.to_string(), font, p.text);
    let size = vec2(g.size().x + 22.0, 28.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let hot = resp.hovered();
    let (fill, stroke, fg) = if primary {
        (if hot { p.text2 } else { p.text }, Stroke::NONE, p.on_inverse)
    } else {
        (if hot { p.hover } else { p.bg }, Stroke::new(1.0, p.border_strong), p.text)
    };
    ui.painter().rect(rect, 6.0, fill, stroke, StrokeKind::Inside);
    ui.painter().galley_with_override_text_color(rect.center() - g.size() / 2.0, g, fg);
    a11y(&resp, egui::Role::Button, label, None);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Full-width primary button used on the sign-in screen.
pub fn wide_button(ui: &mut Ui, label: &str, enabled: bool) -> Response {
    let p = pal(ui);
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::click());
    let hot = enabled && resp.hovered();
    let fill = if !enabled {
        p.text3
    } else if hot {
        p.text2
    } else {
        p.text
    };
    ui.painter().rect_filled(rect, 6.0, fill);
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, label, FontId::new(13.5, medium()), p.on_inverse);
    a11y(&resp, egui::Role::Button, label, None);
    if enabled { resp.on_hover_cursor(egui::CursorIcon::PointingHand) } else { resp }
}

/// Plain text that brightens on hover; for quiet secondary actions.
pub fn text_button(ui: &mut Ui, label: &str, color: Color32) -> Response {
    let p = pal(ui);
    let g = ui.painter().layout_no_wrap(label.to_string(), FontId::proportional(12.5), color);
    let (rect, resp) = ui.allocate_exact_size(g.size() + vec2(0.0, 6.0), Sense::click());
    let c = if resp.hovered() { p.text } else { color };
    ui.painter().galley_with_override_text_color(pos2(rect.left(), rect.top() + 3.0), g, c);
    a11y(&resp, egui::Role::Button, label, None);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Segmented control: a bordered strip where the selected segment is raised.
pub fn segmented(ui: &mut Ui, labels: &[&str], selected: usize) -> Option<usize> {
    let p = pal(ui);
    let font = FontId::new(12.5, medium());
    let widths: Vec<f32> = labels
        .iter()
        .map(|l| ui.painter().layout_no_wrap(l.to_string(), font.clone(), p.text).size().x + 20.0)
        .collect();
    let (rect, _) = ui.allocate_exact_size(vec2(widths.iter().sum::<f32>() + 4.0, 28.0), Sense::hover());
    ui.painter().rect(rect, 6.0, p.bg, Stroke::new(1.0, p.border_strong), StrokeKind::Inside);
    let mut x = rect.left() + 2.0;
    let mut clicked = None;
    for (i, (label, w)) in labels.iter().zip(&widths).enumerate() {
        let seg = Rect::from_min_size(pos2(x, rect.top() + 2.0), vec2(*w, rect.height() - 4.0));
        let resp = ui.interact(seg, ui.scope_id().with(("seg", label)), Sense::click());
        a11y(&resp, egui::Role::RadioButton, label, Some(i == selected));
        if i == selected {
            ui.painter().rect_filled(seg, 4.0, p.hover);
        }
        let col = if i == selected || resp.hovered() { p.text } else { p.text3 };
        ui.painter().text(seg.center(), egui::Align2::CENTER_CENTER, *label, font.clone(), col);
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            clicked = Some(i);
        }
        x += w;
    }
    clicked
}

/// Underline tabs with a hairline across the full width.
pub fn tabs(ui: &mut Ui, labels: &[&str], selected: usize) -> Option<usize> {
    let p = pal(ui);
    let font = FontId::new(13.0, medium());
    let mut clicked = None;
    let top = ui.cursor().top();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 20.0;
        for (i, label) in labels.iter().enumerate() {
            let g = ui.painter().layout_no_wrap(label.to_string(), font.clone(), p.text);
            let (rect, resp) = ui.allocate_exact_size(vec2(g.size().x, 36.0), Sense::click());
            a11y(&resp, egui::Role::Tab, label, Some(i == selected));
            let col = if i == selected || resp.hovered() { p.text } else { p.text3 };
            ui.painter().galley_with_override_text_color(pos2(rect.left(), rect.center().y - g.size().y / 2.0), g, col);
            if i == selected {
                ui.painter().hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.5, p.text));
            }
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                clicked = Some(i);
            }
        }
    });
    let y = top + 36.0;
    ui.painter().hline(ui.max_rect().x_range(), y, Stroke::new(1.0, p.border));
    clicked
}

/// A minimal square checkbox with a label, e.g. "Hide completed".
pub fn toggle(ui: &mut Ui, on: &mut bool, label: &str) -> Response {
    let p = pal(ui);
    let g = ui.painter().layout_no_wrap(label.to_string(), FontId::proportional(12.5), p.text2);
    let (rect, resp) = ui.allocate_exact_size(vec2(g.size().x + 20.0, 28.0), Sense::click());
    let bx = Rect::from_center_size(pos2(rect.left() + 6.0, rect.center().y), Vec2::splat(12.0));
    if *on {
        ui.painter().rect_filled(bx, 3.0, p.text);
        let s = Stroke::new(1.4, p.on_inverse);
        let c = bx.center();
        ui.painter().line_segment([c + vec2(-3.0, 0.0), c + vec2(-0.8, 2.2)], s);
        ui.painter().line_segment([c + vec2(-0.8, 2.2), c + vec2(3.0, -2.2)], s);
    } else {
        let st = if resp.hovered() { p.text2 } else { p.text3 };
        ui.painter().rect_stroke(bx, 3.0, Stroke::new(1.0, st), StrokeKind::Inside);
    }
    let col = if resp.hovered() || *on { p.text } else { p.text2 };
    ui.painter().galley_with_override_text_color(pos2(bx.right() + 8.0, rect.center().y - g.size().y / 2.0), g, col);
    a11y(&resp, egui::Role::CheckBox, label, Some(*on));
    if resp.clicked() {
        *on = !*on;
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Thin progress line with a mono count, e.g. "4/8".
pub fn progress(ui: &mut Ui, done: usize, total: usize) {
    let p = pal(ui);
    ui.label(mono(format!("{done}/{total}"), 12.0, p.text3));
    let (rect, _) = ui.allocate_exact_size(vec2(64.0, 28.0), Sense::hover());
    let track = Rect::from_center_size(rect.center(), vec2(64.0, 2.0));
    ui.painter().rect_filled(track, 1.0, p.border_strong);
    let frac = if total == 0 { 0.0 } else { done as f32 / total as f32 };
    let mut fill = track;
    fill.set_width(track.width() * frac);
    ui.painter().rect_filled(fill, 1.0, p.text);
}

/// Bordered single-line input matching the rest of the system.
pub fn input(ui: &mut Ui, value: &mut String, hint: &str, width: f32, password: bool) -> Response {
    let p = pal(ui);
    let frame = egui::Frame::new()
        .fill(p.bg)
        .stroke(Stroke::new(1.0, p.border_strong))
        .corner_radius(6)
        .inner_margin(egui::Margin::symmetric(10, 7));
    frame
        .show(ui, |ui| {
            ui.add(
                egui::TextEdit::singleline(value)
                    .frame(egui::Frame::NONE)
                    .hint_text(txt(hint, 13.5, p.text3))
                    .password(password)
                    .desired_width(width - 20.0),
            )
        })
        .inner
}

pub fn spinner(ui: &mut Ui, size: f32) {
    ui.add(egui::Spinner::new().size(size).color(pal(ui).text3));
}
