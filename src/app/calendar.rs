//! Calendar: a seven-column week of item blocks, and a month grid.

use super::ui::{header_bar, kind_label, loading};
use super::*;
use crate::theme::{self, Icon, Status, pal};
use eframe::egui::{Align2, FontId, Rect, ScrollArea, Sense, Stroke, StrokeKind, Ui, pos2, vec2};

const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

impl App {
    pub(super) fn calendar(&self, ui: &mut Ui, out: &mut Vec<Action>) {
        let (first, last) = self.cal_range();
        let title = match self.cal_mode {
            CalMode::Week if first.year() != last.year() => {
                format!("{} – {}", first.format("%b %-d, %Y"), last.format("%b %-d, %Y"))
            }
            CalMode::Week if first.month() != last.month() => {
                format!("{} – {}", first.format("%b %-d"), last.format("%b %-d, %Y"))
            }
            CalMode::Week => format!("{} – {}", first.format("%b %-d"), last.format("%-d, %Y")),
            CalMode::Month => self.cal_anchor.format("%B %Y").to_string(),
        };
        let unit = if self.cal_mode == CalMode::Week { "week" } else { "month" };
        header_bar(ui, &[("Calendar", false), (&title, true)], None, |ui| {
            let today = Local::now().date_naive();
            if !(first..=last).contains(&today) && theme::button(ui, "Today", false).clicked() {
                out.push(Action::CalShift(0));
            }
            if theme::icon_button(ui, Icon::ChevronRight, &format!("Next {unit}  ·  →")).clicked() {
                out.push(Action::CalShift(1));
            }
            if theme::icon_button(ui, Icon::ChevronLeft, &format!("Previous {unit}  ·  ←")).clicked() {
                out.push(Action::CalShift(-1));
            }
            ui.add_space(8.0);
            let sel = if self.cal_mode == CalMode::Week { 0 } else { 1 };
            match theme::segmented(ui, &["Week", "Month"], sel) {
                Some(0) => out.push(Action::CalMode(CalMode::Week)),
                Some(1) => out.push(Action::CalMode(CalMode::Month)),
                _ => {}
            }
        });

        // Nothing loaded for this range yet (first launch / far future).
        let any_loaded = {
            let mut d = first;
            let mut found = false;
            while d <= last {
                found |= self.data.planner.contains_key(&week_key(d));
                d += Duration::weeks(1);
            }
            found
        };
        if !any_loaded {
            loading(ui, "Loading calendar");
            return;
        }
        match self.cal_mode {
            CalMode::Week => self.week_grid(ui, out, first),
            CalMode::Month => self.month_grid(ui, out, first, last),
        }
    }

    fn week_grid(&self, ui: &mut Ui, out: &mut Vec<Action>, monday: NaiveDate) {
        let p = pal(ui);
        let today = Local::now().date_naive();
        let area = ui.available_rect_before_wrap();
        let col_w = area.width() / 7.0;
        let head_h = 64.0;

        // Day headers.
        let (head, _) = ui.allocate_exact_size(vec2(area.width(), head_h), Sense::hover());
        for i in 0..7 {
            let day = monday + Duration::days(i);
            let x = head.left() + col_w * i as f32;
            let past = day < today;
            let wd = theme::tracked(
                ui,
                &WEEKDAYS[i as usize].to_uppercase(),
                FontId::new(theme::size::XS, theme::medium()),
                p.text3,
                0.8,
            );
            ui.painter().galley(pos2(x + 14.0, head.top() + 18.0 - wd.size().y / 2.0), wd, p.text3);
            let num = day.day().to_string();
            let font = FontId::new(20.0, theme::semibold());
            if day == today {
                let g = ui.painter().layout_no_wrap(num.clone(), font.clone(), p.on_inverse);
                let r = Rect::from_min_size(pos2(x + 8.0, head.top() + 30.0), vec2(g.size().x.max(14.0) + 14.0, 28.0));
                ui.painter().rect_filled(r, 6.0, p.text);
                ui.painter().galley(r.center() - g.size() / 2.0, g, p.on_inverse);
            } else {
                ui.painter().text(
                    pos2(x + 14.0, head.top() + 44.0),
                    Align2::LEFT_CENTER,
                    num,
                    font,
                    if past { p.text3 } else { p.text },
                );
            }
        }
        ui.painter().hline(head.x_range(), head.bottom() - 0.5, Stroke::new(1.0, p.border));

        let body_top = head.bottom();
        // Column rules run the full height of the view.
        for i in 1..7 {
            let x = area.left() + col_w * i as f32;
            ui.painter().vline(x, area.top()..=area.bottom(), Stroke::new(1.0, p.border));
        }

        ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            let mut max_h: f32 = 0.0;
            let top = ui.cursor().top();
            for i in 0..7 {
                let day = monday + Duration::days(i);
                let (week, items) = self.day_items(day);
                let x = area.left() + col_w * i as f32;
                let mut y = top + 10.0;
                for e in self.syllabus_exams_on(day) {
                    let h = self.exam_block(ui, out, e, Rect::from_min_size(pos2(x + 6.0, y), vec2(col_w - 12.0, 0.0)));
                    y += h + 6.0;
                }
                for it in items {
                    let h = self.week_block(
                        ui,
                        out,
                        it,
                        &week,
                        Rect::from_min_size(pos2(x + 6.0, y), vec2(col_w - 12.0, 0.0)),
                    );
                    y += h + 6.0;
                }
                max_h = max_h.max(y - top);
            }
            ui.allocate_space(vec2(area.width(), max_h.max(ui.available_height()).max(body_top - top)));
        });
    }

    /// Draws one item block at `at` (height is computed) and returns its height.
    fn week_block(&self, ui: &mut Ui, out: &mut Vec<Action>, it: &PlannerItem, week: &str, at: Rect) -> f32 {
        let p = pal(ui);
        let w = at.width();
        let pad = 9.0;
        let done = it.done();
        let exam = self.item_exam(it);
        let title_color = if done { p.text3 } else { p.text };
        let title = exam.map(|e| e.label.clone()).unwrap_or_else(|| crate::names::clean_title(it.title()));
        let mut job =
            egui::text::LayoutJob::simple(title, FontId::new(12.5, theme::medium()), title_color, w - pad * 2.0 - 18.0);
        job.wrap.max_rows = 3;
        let g = ui.painter().layout_job(job);
        let tag_h = if exam.is_some() { 16.0 } else { 0.0 };
        let h = pad + tag_h + g.size().y + 6.0 + 14.0 + pad;
        let rect = Rect::from_min_size(at.min, vec2(w, h));
        let id = egui::Id::unique(("cal", it.plannable_type.as_str(), it.plannable_id));
        let resp = ui.interact(rect, id, Sense::click());
        theme::a11y(&resp, egui::Role::Button, it.title(), None);
        let hot = resp.hovered();
        // Exams get a high-contrast outline: emphasis without adding a colour.
        let edge = match (exam.is_some() && !done, hot) {
            (true, _) => p.text2,
            (false, true) => p.border_strong,
            (false, false) => p.border,
        };
        ui.painter().rect(rect, 6.0, if hot { p.hover } else { p.raised }, Stroke::new(1.0, edge), StrokeKind::Inside);
        if exam.is_some() {
            ui.painter().text(
                pos2(rect.left() + pad + 2.0, rect.top() + pad + 5.0),
                Align2::LEFT_CENTER,
                "EXAM",
                FontId::new(10.0, theme::mono_medium()),
                if done { p.text3 } else { p.text },
            );
        }
        let rect_body = rect.translate(vec2(0.0, tag_h));
        // Course identity: a thin inset bar on the left edge.
        let color = self.color(it.course_id);
        let bar = Rect::from_min_size(rect.min + vec2(0.0, 6.0), vec2(2.0, h - 12.0));
        ui.painter().rect_filled(bar, 1.0, if done { color.gamma_multiply(0.4) } else { color });

        let status = if done {
            Status::Done
        } else if it.missing() {
            Status::Missing
        } else {
            Status::Todo
        };
        let can_toggle = !it.submitted() && !it.graded() && !self.toggling.contains(&it.plannable_id);
        let circle = pos2(rect.left() + pad + 6.0, rect_body.top() + pad + 8.0);
        if theme::status_circle(ui, circle, status, can_toggle, id.with("st")).clicked() {
            out.push(Action::Toggle { week: week.to_string(), plannable_id: it.plannable_id });
        }
        ui.painter().galley(pos2(rect.left() + pad + 18.0, rect_body.top() + pad), g.clone(), title_color);

        // Compact "11:59p" so time and course fit a narrow column. Exams show their start time.
        let t = it.plannable_date.with_timezone(&Local);
        let time = compact_time(exam.and_then(|e| e.time).unwrap_or(t.time()));
        let course = self.course_label(it.course_id, it.context_name.as_deref());
        let meta_y = rect_body.top() + pad + g.size().y + 6.0 + 7.0;
        let (meta, meta_color) = if it.missing() && !done {
            (format!("Missing · {course}"), p.danger)
        } else {
            (format!("{time} · {course}"), p.text3)
        };
        let mg = meta_line(ui, &meta, meta_color, w - pad * 2.0 - 2.0);
        ui.painter().galley(pos2(rect.left() + pad + 2.0, meta_y - mg.size().y / 2.0), mg, meta_color);

        let tip = [kind_label(&it.plannable_type).unwrap_or("Assignment"), it.title()].join(": ");
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(tip).clicked()
            && let Some(u) = &it.html_url
        {
            out.push(Action::Open(u.clone()));
        }
        h
    }

    /// A syllabus-only exam in the week grid (no Canvas item to tick off).
    fn exam_block(&self, ui: &mut Ui, out: &mut Vec<Action>, e: &Exam, at: Rect) -> f32 {
        let p = pal(ui);
        let w = at.width();
        let pad = 9.0;
        let mut job =
            egui::text::LayoutJob::simple(e.label.clone(), FontId::new(12.5, theme::medium()), p.text, w - pad * 2.0);
        job.wrap.max_rows = 2;
        let g = ui.painter().layout_job(job);
        let h = pad + 16.0 + g.size().y + 6.0 + 14.0 + pad;
        let rect = Rect::from_min_size(at.min, vec2(w, h));
        let resp = ui.interact(rect, egui::Id::unique(("exam", e.course_id, e.date)), Sense::click());
        theme::a11y(&resp, egui::Role::Button, &e.label, None);
        let hot = resp.hovered();
        ui.painter().rect(
            rect,
            6.0,
            if hot { p.hover } else { p.raised },
            Stroke::new(1.0, p.text2),
            StrokeKind::Inside,
        );
        let color = self.color(Some(e.course_id));
        ui.painter().rect_filled(Rect::from_min_size(rect.min + vec2(0.0, 6.0), vec2(2.0, h - 12.0)), 1.0, color);
        ui.painter().text(
            pos2(rect.left() + pad + 2.0, rect.top() + pad + 5.0),
            Align2::LEFT_CENTER,
            "EXAM",
            FontId::new(10.0, theme::mono_medium()),
            p.text,
        );
        ui.painter().galley(pos2(rect.left() + pad + 2.0, rect.top() + pad + 16.0), g.clone(), p.text);
        let time = e.time.map(compact_time);
        let course = self.course_label(Some(e.course_id), None);
        let meta = match time {
            _ if e.approx => format!("this week · {course}"),
            Some(t) => format!("{t} · {course}"),
            None => course,
        };
        let mg = meta_line(ui, &meta, p.text3, w - pad * 2.0 - 2.0);
        let meta_y = rect.top() + pad + 16.0 + g.size().y + 6.0 + 7.0;
        ui.painter().galley(pos2(rect.left() + pad + 2.0, meta_y - mg.size().y / 2.0), mg, p.text3);
        if resp
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text("From the syllabus. Click to open it.")
            .clicked()
        {
            out.push(Action::CourseTab(e.course_id, Tab::Syllabus));
        }
        h
    }

    fn month_grid(&self, ui: &mut Ui, out: &mut Vec<Action>, first: NaiveDate, last: NaiveDate) {
        let p = pal(ui);
        let today = Local::now().date_naive();
        let month = self.cal_anchor.month();
        let area = ui.available_rect_before_wrap();
        let col_w = area.width() / 7.0;
        let weeks = ((last - first).num_days() + 1) / 7;

        let (head, _) = ui.allocate_exact_size(vec2(area.width(), 32.0), Sense::hover());
        for (i, d) in WEEKDAYS.iter().enumerate() {
            let wd = theme::tracked(ui, &d.to_uppercase(), FontId::new(theme::size::XS, theme::medium()), p.text3, 0.8);
            ui.painter().galley(
                pos2(head.left() + col_w * i as f32 + 12.0, head.center().y - wd.size().y / 2.0),
                wd,
                p.text3,
            );
        }
        ui.painter().hline(head.x_range(), head.bottom() - 0.5, Stroke::new(1.0, p.border));

        let avail = ui.available_height();
        let cell_h = (avail / weeks as f32).max(104.0).floor();
        ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            let (grid, _) = ui.allocate_exact_size(vec2(area.width(), cell_h * weeks as f32), Sense::hover());
            for w in 0..weeks {
                for i in 0..7 {
                    let day = first + Duration::days(w * 7 + i);
                    let cell = Rect::from_min_size(
                        pos2(grid.left() + col_w * i as f32, grid.top() + cell_h * w as f32),
                        vec2(col_w, cell_h),
                    );
                    self.month_cell(ui, out, day, cell, day.month() == month, day == today);
                }
            }
            // Rules last so hover fills sit beneath them.
            for i in 1..7 {
                ui.painter().vline(grid.left() + col_w * i as f32, grid.y_range(), Stroke::new(1.0, p.border));
            }
            for w in 1..=weeks {
                ui.painter().hline(grid.x_range(), grid.top() + cell_h * w as f32 - 0.5, Stroke::new(1.0, p.border));
            }
        });
    }

    fn month_cell(
        &self,
        ui: &mut Ui,
        out: &mut Vec<Action>,
        day: NaiveDate,
        cell: Rect,
        in_month: bool,
        is_today: bool,
    ) {
        let p = pal(ui);
        let resp = ui.interact(cell, egui::Id::unique(("cell", day)), Sense::click());
        theme::a11y(&resp, egui::Role::Button, &day.format("%A %B %-d").to_string(), None);
        if resp.hovered() {
            ui.painter().rect_filled(cell, 0.0, p.raised);
        }
        let num = day.day().to_string();
        let font = FontId::monospace(12.0);
        let num_pos = pos2(cell.left() + 10.0, cell.top() + 9.0);
        if is_today {
            let g = ui.painter().layout_no_wrap(num.clone(), FontId::new(12.0, theme::mono_medium()), p.on_inverse);
            let r = Rect::from_min_size(num_pos - vec2(5.0, 3.0), vec2(g.size().x + 10.0, g.size().y + 6.0));
            ui.painter().rect_filled(r, 4.0, p.text);
            ui.painter().galley(num_pos, g, p.on_inverse);
        } else {
            let c = if !in_month {
                p.text4
            } else if day < Local::now().date_naive() {
                p.text3
            } else {
                p.text2
            };
            ui.painter().text(num_pos, Align2::LEFT_TOP, num, font, c);
        }

        let (_, items) = self.day_items(day);
        let exams = self.syllabus_exams_on(day);
        let line_h = 19.0;
        let top = cell.top() + 32.0;
        let fits = (((cell.bottom() - 6.0 - top) / line_h).floor() as usize).max(1);
        for (k, e) in exams.iter().take(fits).enumerate() {
            let y = top + line_h * k as f32;
            let r = Rect::from_min_size(pos2(cell.left() + 4.0, y), vec2(cell.width() - 8.0, line_h - 2.0));
            let ir = ui.interact(r, egui::Id::unique(("mexam", e.course_id, e.date)), Sense::click());
            theme::a11y(&ir, egui::Role::Link, &e.label, None);
            if ir.hovered() {
                ui.painter().rect_filled(r, 4.0, p.hover);
            }
            // Outlined square = from the syllabus (no Canvas item behind it).
            let sq = Rect::from_center_size(pos2(r.left() + 7.0, r.center().y), vec2(6.0, 6.0));
            ui.painter().rect_stroke(sq, 1.5, Stroke::new(1.2, self.color(Some(e.course_id))), StrokeKind::Inside);
            let g = theme::galley(ui, &e.label, FontId::new(12.0, theme::medium()), p.text, r.width() - 22.0);
            ui.painter().galley(pos2(r.left() + 16.0, r.center().y - g.size().y / 2.0), g, p.text);
            let course = self.course_label(Some(e.course_id), None);
            let when = e.time.map(|t| t.format("%-I:%M %p").to_string()).unwrap_or_else(|| "time not stated".into());
            if ir
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text(format!("{}  ·  {course}\n{when}  ·  from syllabus", e.label))
                .clicked()
            {
                out.push(Action::CourseTab(e.course_id, Tab::Syllabus));
            }
        }
        let fits = fits.saturating_sub(exams.len()).max(1);
        let shown = if items.len() > fits { fits - 1 } else { items.len() };
        let base = exams.len();
        for (k, it) in items.iter().take(shown).enumerate() {
            let y = top + line_h * (base + k) as f32;
            let r = Rect::from_min_size(pos2(cell.left() + 4.0, y), vec2(cell.width() - 8.0, line_h - 2.0));
            let ir = ui.interact(
                r,
                egui::Id::unique(("mitem", it.plannable_type.as_str(), it.plannable_id)),
                Sense::click(),
            );
            theme::a11y(&ir, egui::Role::Link, it.title(), None);
            if ir.hovered() {
                ui.painter().rect_filled(r, 4.0, p.hover);
            }
            let done = it.done();
            let color = self.color(it.course_id);
            theme::course_square(
                ui,
                pos2(r.left() + 7.0, r.center().y),
                if done { color.gamma_multiply(0.35) } else { color },
                6.0,
            );
            let exam = self.item_exam(it);
            let is_exam = exam.is_some();
            let fg = if done {
                p.text3
            } else if is_exam {
                p.text
            } else if it.missing() {
                p.danger
            } else if ir.hovered() {
                p.text
            } else {
                p.text2
            };
            let font = if is_exam { FontId::new(12.0, theme::medium()) } else { FontId::proportional(12.0) };
            let label = exam.map(|e| e.label.clone()).unwrap_or_else(|| crate::names::clean_title(it.title()));
            let g = theme::galley(ui, &label, font, fg, r.width() - 22.0);
            ui.painter().galley(pos2(r.left() + 16.0, r.center().y - g.size().y / 2.0), g, fg);
            let time = it.plannable_date.with_timezone(&Local).format("%-I:%M %p");
            let course = self.course_label(it.course_id, it.context_name.as_deref());
            if ir
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text(format!("{}\n{course}  ·  {time}", it.title()))
                .clicked()
                && let Some(u) = &it.html_url
            {
                out.push(Action::Open(u.clone()));
            }
        }
        if items.len() > shown {
            let y = top + line_h * (base + shown) as f32 + line_h / 2.0;
            ui.painter().text(
                pos2(cell.left() + 20.0, y - 1.0),
                Align2::LEFT_CENTER,
                format!("+{} more", items.len() - shown),
                FontId::proportional(11.5),
                p.text3,
            );
        }
        // Clicking empty space (or "+N more") opens that week.
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            out.push(Action::CalGoto(day, CalMode::Week));
        }
    }
}

/// "11:59p · COMP 222": the time in mono (so times align), the rest proportional
/// (so course codes fit), truncated as one line.
fn meta_line(ui: &Ui, meta: &str, color: Color32, max_w: f32) -> std::sync::Arc<egui::Galley> {
    let (time, rest) = match meta.split_once(" · ") {
        Some((t, r)) if t.chars().next().is_some_and(|c| c.is_ascii_digit()) => (t, format!("  ·  {r}")),
        _ => ("", meta.to_string()),
    };
    let mut job = egui::text::LayoutJob::default();
    let fmt = |font: FontId| egui::TextFormat { font_id: font, color, ..Default::default() };
    job.append(time, 0.0, fmt(FontId::monospace(11.0)));
    job.append(&rest, 0.0, fmt(FontId::proportional(11.5)));
    job.wrap = egui::text::TextWrapping::truncate_at_width(max_w.max(8.0));
    ui.painter().layout_job(job)
}
