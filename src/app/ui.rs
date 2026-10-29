//! Rendering for everything except the calendar grids.

use super::*;
use crate::api::{Announcement, Module};
use crate::theme::{self, Icon, Pal, Status, med, mono, pal, semi, size, txt};
use eframe::egui::{Align, Align2, FontId, Frame, Layout, Margin, Rect, ScrollArea, Sense, Stroke, Ui, pos2, vec2};

/// One horizontal inset for every page: header text, rows and content line up.
pub(super) const PAD: f32 = 28.0;
const ROW_H: f32 = 40.0;
const HEADER_H: f32 = 52.0;

impl App {
    pub(super) fn frame(&mut self, ui: &mut Ui, out: &mut Vec<Action>) {
        let p = pal(ui);
        if self.api.is_some() {
            egui::Panel::left("side")
                .resizable(false)
                .exact_size(236.0)
                .frame(Frame::new().fill(p.sidebar).inner_margin(Margin { left: 10, right: 10, top: 0, bottom: 10 }))
                .show(ui, |ui| self.sidebar(ui, out));
        }
        egui::CentralPanel::default_margins().frame(Frame::new().fill(p.bg)).show(ui, |ui| {
            if self.session == Session::Expired && self.api.is_some() {
                session_banner(ui, out);
            }
            match self.view {
                View::Settings if self.api.is_none() => self.sign_in(ui),
                View::Settings => self.settings(ui),
                View::Upcoming => {
                    let mut hide = self.hide_done;
                    self.upcoming(ui, out, &mut hide);
                    self.hide_done = hide;
                }
                View::Calendar => self.calendar(ui, out),
                View::Course(id) => {
                    let mut filter = std::mem::take(&mut self.syllabus_filter);
                    self.course_view(ui, out, id, &mut filter);
                    self.syllabus_filter = filter;
                }
            }
        });
    }

    // ---------- sidebar ----------

    fn sidebar(&self, ui: &mut Ui, out: &mut Vec<Action>) {
        let p = pal(ui);
        // Room for the macOS traffic lights; also a window drag handle.
        drag_region(ui, 38.0);
        ui.horizontal(|ui| {
            ui.add_space(6.0);
            let (r, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
            logo(ui, r);
            ui.add_space(2.0);
            ui.label(med("Canvas", 14.0, p.text));
            ui.label(txt("Lite", 14.0, p.text3));
        });
        ui.add_space(14.0);

        let left = self.week_items(0).map(|v| v.iter().filter(|i| !i.done()).count()).unwrap_or(0);
        let count = (left > 0).then(|| left.to_string());
        if nav_item(ui, Some(Icon::List), None, "Upcoming", count.as_deref(), self.view == View::Upcoming)
            .on_hover_text("U")
            .clicked()
        {
            out.push(Action::View(View::Upcoming));
        }
        if nav_item(ui, Some(Icon::Calendar), None, "Calendar", None, self.view == View::Calendar)
            .on_hover_text("C")
            .clicked()
        {
            out.push(Action::View(View::Calendar));
        }

        ui.add_space(18.0);
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            ui.label(med("Courses", 11.5, p.text3));
        });
        ui.add_space(2.0);
        for (i, c) in self.data.courses.iter().enumerate() {
            let grade = c.score().filter(|_| !self.prefs.hide_grades).map(|s| format!("{s:.1}"));
            let sel = self.view == View::Course(c.id);
            let r = nav_item(ui, None, Some(self.color(Some(c.id))), &c.short(), grade.as_deref(), sel);
            let r = if i < 9 {
                r.on_hover_text(format!("{}  ·  {}", c.title(), i + 1))
            } else {
                r.on_hover_text(c.title())
            };
            if r.clicked() {
                out.push(Action::View(View::Course(c.id)));
            }
        }
        if self.data.courses.is_empty() {
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                theme::spinner(ui, 12.0);
            });
        }

        ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
            self.sync_status(ui, out);
            if nav_item(ui, Some(Icon::Settings), None, "Settings", None, self.view == View::Settings).clicked() {
                out.push(Action::View(View::Settings));
            }
        });
    }

    fn sync_status(&self, ui: &mut Ui, out: &mut Vec<Action>) {
        let p = pal(ui);
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::click());
        let dot = pos2(rect.left() + 13.0, rect.center().y);
        let renewing = self.session == Session::Renewing;
        let (label, color) = if renewing {
            ("Reconnecting…".to_string(), p.text3)
        } else if self.session == Session::Expired {
            ("Signed out · sign in".to_string(), p.danger)
        } else if self.inflight > 0 {
            ("Syncing…".to_string(), p.text3)
        } else if self.error.is_some() {
            ("Sync failed · retry".to_string(), p.danger)
        } else {
            let when = self.data.updated_at.map(ago).unwrap_or_else(|| "never".into());
            (format!("Synced {when}"), p.text3)
        };
        if self.inflight > 0 || renewing {
            // A slow pulse instead of a spinner: quieter in peripheral vision.
            let t = ui.input(|i| i.time) as f32;
            let a = 0.35 + 0.65 * (t * 3.0).sin().abs();
            ui.painter().circle_filled(dot, 3.0, p.text2.gamma_multiply(a));
            ui.ctx().request_repaint();
        } else {
            let bad = self.error.is_some() || self.session == Session::Expired;
            ui.painter().circle_filled(dot, 3.0, if bad { p.danger } else { p.text4 });
        }
        let c = if resp.hovered() { p.text } else { color };
        ui.painter().text(
            pos2(rect.left() + 26.0, rect.center().y),
            Align2::LEFT_CENTER,
            &label,
            FontId::proportional(12.0),
            c,
        );
        theme::a11y(&resp, egui::Role::Button, &label, None);
        let mut resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
        resp = match &self.error {
            Some(e) => resp.on_hover_text(e),
            None => resp.on_hover_text("Refresh  ·  R"),
        };
        if resp.clicked() {
            out.push(if self.session == Session::Expired { Action::Reauth } else { Action::Refresh });
        }
    }

    // ---------- upcoming ----------

    fn upcoming(&self, ui: &mut Ui, out: &mut Vec<Action>, hide_done: &mut bool) {
        let start = week_start(self.week_offset);
        let end = start + Duration::days(6);
        let key = week_key(start);
        let items = self.week_items(self.week_offset);
        let title = match self.week_offset {
            0 => "This week".to_string(),
            1 => "Next week".to_string(),
            -1 => "Last week".to_string(),
            _ => format!("Week of {}", start.format("%b %-d")),
        };
        let range = format!("{} – {}", start.format("%b %-d"), end.format("%b %-d"));
        header_bar(ui, &[("Upcoming", false), (&title, true)], Some(&range), |ui| {
            if self.week_offset != 0 && theme::button(ui, "Today", false).clicked() {
                out.push(Action::ShiftWeek(0));
            }
            if theme::icon_button(ui, Icon::ChevronRight, "Next week  ·  →").clicked() {
                out.push(Action::ShiftWeek(1));
            }
            if theme::icon_button(ui, Icon::ChevronLeft, "Previous week  ·  ←").clicked() {
                out.push(Action::ShiftWeek(-1));
            }
            ui.add_space(10.0);
            theme::toggle(ui, hide_done, "Hide completed");
            if let Some(items) = items.filter(|v| !v.is_empty()) {
                ui.add_space(10.0);
                theme::progress(ui, items.iter().filter(|i| i.done()).count(), items.len());
            }
        });

        ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            if self.week_offset == 0 {
                self.overdue(ui, out, start);
                self.next_exams(ui, out);
            }
            let Some(items) = items else {
                loading(ui, "Loading week");
                return;
            };
            let today = Local::now().date_naive();
            for d in 0..7 {
                let day = start + Duration::days(d);
                let mut todays: Vec<&PlannerItem> =
                    items.iter().filter(|i| i.plannable_date.with_timezone(&Local).date_naive() == day).collect();
                todays.sort_by_key(|i| i.plannable_date);
                let total = todays.len();
                let open = todays.iter().filter(|i| !i.done()).count();
                if *hide_done {
                    todays.retain(|i| !i.done());
                }
                let name = if day == today { "Today".to_string() } else { day.format("%A").to_string() };
                let count = if total == 0 { String::new() } else { format!("{open}/{total}") };
                let exams = self.syllabus_exams_on(day);
                group_header(
                    ui,
                    &name,
                    &day.format("%b %-d").to_string(),
                    &count,
                    total == 0 && exams.is_empty(),
                    None,
                );
                for e in exams {
                    self.exam_row(ui, out, e, false, true);
                }
                for it in todays {
                    self.planner_row(ui, out, it, &key);
                }
            }
            ui.add_space(40.0);
        });
    }

    fn overdue(&self, ui: &mut Ui, out: &mut Vec<Action>, week_start: NaiveDate) {
        let p = pal(ui);
        let cutoff = local_midnight(week_start);
        let mut late: Vec<&Assignment> =
            self.data.missing.iter().filter(|a| a.due_at.is_some_and(|d| d < cutoff)).collect();
        if late.is_empty() {
            return;
        }
        late.sort_by_key(|a| a.due_at);
        group_header(ui, "Overdue", "", &late.len().to_string(), false, Some(p.danger));
        for a in late {
            let due = a.due_at.map(|d| d.with_timezone(&Local).format("%b %-d").to_string()).unwrap_or_default();
            let row = Row {
                id: egui::Id::unique(("overdue", a.id)),
                status: Some((Status::Missing, false)),
                title: &a.name,
                dim: false,
                course: Some((self.course_label(a.course_id, None), self.color(a.course_id), a.course_id)),
                widths: COLS,
                cols: [Col::text("Missing", p.danger), Col::empty(), Col::mono(&due, p.text2)],
            };
            let r = row.show(ui, out);
            if r.title_clicked
                && let Some(u) = &a.html_url
            {
                out.push(Action::Open(u.clone()));
            }
        }
    }

    /// Exams in the next five weeks, so they're visible before they're "this week".
    fn next_exams(&self, ui: &mut Ui, out: &mut Vec<Action>) {
        let today = Local::now().date_naive();
        let soon: Vec<&Exam> =
            self.exams.iter().filter(|e| e.date >= today && e.date <= today + Duration::days(35)).collect();
        if soon.is_empty() {
            return;
        }
        group_header(ui, "Exams", "next 5 weeks", &soon.len().to_string(), false, None);
        for e in soon {
            self.exam_row(ui, out, e, true, true);
        }
    }

    /// `countdown`: show "in N days" + full date (exam list) vs. just the time (inside a day).
    /// `show_course`: off inside the exam's own course page.
    fn exam_row(&self, ui: &mut Ui, out: &mut Vec<Action>, e: &Exam, countdown: bool, show_course: bool) {
        let p = pal(ui);
        let days = (e.date - Local::now().date_naive()).num_days();
        let rel = match days {
            0 => "today".to_string(),
            1 => "tomorrow".to_string(),
            n if n < 0 => format!("{} days ago", -n),
            n => format!("in {n} days"),
        };
        let time = e.time.map(|t| t.format("%-I:%M %p").to_string());
        let when = if e.approx {
            format!("Week of {}", e.date.format("%b %-d"))
        } else if countdown {
            match &time {
                Some(t) => format!("{}, {t}", e.date.format("%a %b %-d")),
                None => e.date.format("%a %b %-d").to_string(),
            }
        } else {
            time.unwrap_or_default()
        };
        let rel = if days < 0 { String::new() } else { rel };
        let row = Row {
            id: egui::Id::unique(("exam-row", e.course_id, e.date, countdown)),
            status: Some((Status::Exam { past: days < 0 }, false)),
            title: &e.label,
            dim: days < 0,
            course: show_course.then(|| {
                (self.course_label(Some(e.course_id), None), self.color(Some(e.course_id)), Some(e.course_id))
            }),
            widths: if countdown { [84.0, 96.0, 150.0] } else { COLS },
            cols: [
                Col::empty(),
                Col::text(
                    if countdown { &rel } else { "Exam" },
                    if countdown && (0..=1).contains(&days) { p.text } else { p.text3 },
                ),
                Col::mono(&when, p.text2),
            ],
        };
        let r = row.show(ui, out);
        if r.title_clicked {
            match &e.url {
                Some(u) => out.push(Action::Open(u.clone())),
                None => out.push(Action::CourseTab(e.course_id, Tab::Syllabus)),
            }
        }
    }

    fn planner_row(&self, ui: &mut Ui, out: &mut Vec<Action>, it: &PlannerItem, week: &str) {
        let p = pal(ui);
        let status = if it.done() {
            Status::Done
        } else if it.missing() {
            Status::Missing
        } else {
            Status::Todo
        };
        let can_toggle = !it.submitted() && !it.graded() && !self.toggling.contains(&it.plannable_id);
        let (word, wc) = item_status(it, &p);
        let pts = it
            .plannable
            .points_possible
            .filter(|p| *p > 0.0)
            .map(|v| format!("{} pts", fmt_num(v)))
            .unwrap_or_default();
        let time = it.plannable_date.with_timezone(&Local).format("%-I:%M %p").to_string();
        let is_exam = self.item_exam(it).is_some();
        let kind = if is_exam { "Exam" } else { kind_label(&it.plannable_type).unwrap_or("") };
        let row = Row {
            id: egui::Id::unique(("item", it.plannable_type.as_str(), it.plannable_id)),
            status: Some((status, can_toggle)),
            title: it.title(),
            dim: it.done(),
            course: Some((
                self.course_label(it.course_id, it.context_name.as_deref()),
                self.color(it.course_id),
                it.course_id,
            )),
            widths: COLS,
            cols: [
                Col::text(
                    if word.is_empty() { kind } else { word },
                    if !word.is_empty() {
                        wc
                    } else if is_exam {
                        p.text
                    } else {
                        p.text3
                    },
                ),
                Col::mono(&pts, p.text3),
                Col::mono(&time, p.text2),
            ],
        };
        let r = row.show(ui, out);
        if r.status_clicked {
            out.push(Action::Toggle { week: week.to_string(), plannable_id: it.plannable_id });
        }
        if r.title_clicked
            && let Some(u) = &it.html_url
        {
            out.push(Action::Open(u.clone()));
        }
    }

    // ---------- course ----------

    fn course_view(&self, ui: &mut Ui, out: &mut Vec<Action>, id: u64, filter: &mut String) {
        let p = pal(ui);
        let Some(c) = self.course(id) else {
            loading(ui, "Loading course");
            return;
        };
        let short = c.short();
        header_bar(ui, &[("Courses", false), (&short, true)], None, |ui| {
            if theme::icon_button(ui, Icon::External, "Open in Canvas").clicked() {
                out.push(Action::Open(format!("/courses/{id}")));
            }
        });

        let color = self.color(Some(id));
        let d = self.data.details.get(&id);
        ScrollArea::vertical().auto_shrink(false).id_salt(("course", id, self.tab as u8)).show(ui, |ui| {
            Frame::new().inner_margin(Margin { left: PAD as i8, right: PAD as i8, top: 28, bottom: 40 }).show(
                ui,
                |ui| {
                    ui.set_max_width(ui.available_width().min(960.0));
                    // Hero: code, title, term — grade on the right.
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                let (r, _) = ui.allocate_exact_size(vec2(12.0, 30.0), Sense::hover());
                                theme::course_square(ui, r.center(), color, 10.0);
                                ui.label(semi(&short, size::XL, p.text)).on_hover_text(c.full_name());
                            });
                            let title = c.title();
                            let mut sub = Vec::new();
                            if title != short {
                                sub.push(title);
                            }
                            sub.extend(c.term());
                            ui.label(txt(sub.join("  ·  "), 14.0, p.text2));
                        });
                        ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                            if c.score().is_some() && self.grades_hidden(id) {
                                ui.vertical(|ui| {
                                    ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                                        ui.label(semi("••••", size::XL, p.text4));
                                    });
                                    ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                                        if theme::text_button(ui, "Show grade", p.text3).clicked() {
                                            out.push(Action::RevealGrade(id));
                                        }
                                    });
                                });
                            } else if let Some(s) = c.score() {
                                ui.vertical(|ui| {
                                    ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                                        ui.label(semi(format!("{s:.1}%"), size::XL, p.text));
                                    });
                                    let note = match c.grade() {
                                        Some(g) => format!("{g}  ·  Current grade"),
                                        None => "Current grade".to_string(),
                                    };
                                    ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                                        ui.label(txt(note, 12.5, p.text3));
                                    });
                                });
                            }
                        });
                    });
                    ui.add_space(20.0);
                    let names = ["Overview", "Grades", "Assignments", "Announcements", "Modules", "Syllabus"];
                    let all =
                        [Tab::Overview, Tab::Grades, Tab::Assignments, Tab::Announcements, Tab::Modules, Tab::Syllabus];
                    let sel = all.iter().position(|t| *t == self.tab).unwrap_or(0);
                    if let Some(i) = theme::tabs(ui, &names, sel) {
                        out.push(Action::Tab(all[i]));
                    }
                    ui.add_space(8.0);
                    let Some(d) = d else {
                        loading(ui, "Loading course");
                        return;
                    };
                    match self.tab {
                        Tab::Overview => self.overview(ui, out, id, d),
                        Tab::Grades => self.grades(ui, out, id, d),
                        Tab::Assignments => assignments(ui, out, d, self.prefs.hide_scores),
                        Tab::Announcements => {
                            if d.announcements.is_empty() {
                                empty(ui, "No announcements.");
                            }
                            for a in &d.announcements {
                                announcement(ui, out, a, None);
                            }
                        }
                        Tab::Modules => modules(ui, out, d),
                        Tab::Syllabus => self.syllabus_tab(ui, out, id, d, filter),
                    }
                },
            );
        });
    }

    fn overview(&self, ui: &mut Ui, out: &mut Vec<Action>, id: u64, d: &CourseDetail) {
        let now = Utc::now();
        let hide = self.prefs.hide_scores;
        ui.spacing_mut().item_spacing.y = 0.0;
        let exams: Vec<&Exam> = self.exams.iter().filter(|e| e.course_id == id).collect();
        if !exams.is_empty() {
            section(ui, "Exams", exams.len());
            for e in exams {
                self.exam_row(ui, out, e, true, false);
            }
        }
        let upcoming: Vec<&Assignment> =
            d.assignments.iter().filter(|a| a.due_at.is_some_and(|t| t >= now) && !submitted(a)).take(6).collect();
        section(ui, "Up next", upcoming.len());
        if upcoming.is_empty() {
            empty(ui, "Nothing due. You're caught up.");
        }
        for a in upcoming {
            assignment_row(ui, out, a, hide);
        }

        let missing: Vec<&Assignment> =
            d.assignments.iter().filter(|a| a.submission.as_ref().is_some_and(|s| s.missing)).collect();
        if !missing.is_empty() {
            section(ui, "Missing", missing.len());
            for a in missing {
                assignment_row(ui, out, a, hide);
            }
        }

        if !d.announcements.is_empty() {
            section(ui, "Announcements", d.announcements.len());
            for a in d.announcements.iter().take(3) {
                announcement(ui, out, a, Some(220));
            }
        }

        let mut graded: Vec<&Assignment> =
            d.assignments.iter().filter(|a| a.submission.as_ref().is_some_and(|s| s.score.is_some())).collect();
        graded.sort_by_key(|a| std::cmp::Reverse(a.due_at));
        if !graded.is_empty() && !hide {
            section(ui, "Recently graded", graded.len().min(5));
            for a in graded.into_iter().take(5) {
                assignment_row(ui, out, a, hide);
            }
        }
    }

    /// Grades by assignment group, with weights and per-group percentages.
    fn grades(&self, ui: &mut Ui, out: &mut Vec<Action>, id: u64, d: &CourseDetail) {
        let p = pal(ui);
        let hide_total = self.grades_hidden(id);
        let hide_scores = self.prefs.hide_scores;
        ui.spacing_mut().item_spacing.y = 0.0;
        if hide_scores {
            ui.add_space(16.0);
            ui.label(txt("Assignment scores are hidden. Turn them back on in Settings → Privacy.", 13.0, p.text3));
        }
        let counted = |a: &&Assignment| {
            !a.omit_from_final_grade
                && a.points_possible.is_some_and(|pp| pp > 0.0)
                && a.submission.as_ref().is_some_and(|s| s.score.is_some() && s.excused != Some(true))
        };
        let pct = |list: &[&Assignment]| {
            let (got, max) = list.iter().filter(|a| counted(a)).fold((0.0, 0.0), |(g, m), a| {
                (g + a.submission.as_ref().and_then(|s| s.score).unwrap_or(0.0), m + a.points_possible.unwrap_or(0.0))
            });
            (max > 0.0).then(|| got / max * 100.0)
        };
        let mut groups: Vec<(String, Option<f64>, Vec<&Assignment>)> = d
            .groups
            .iter()
            .map(|g| {
                (
                    g.name.clone(),
                    g.group_weight,
                    d.assignments.iter().filter(|a| a.assignment_group_id == Some(g.id)).collect(),
                )
            })
            .collect();
        let loose: Vec<&Assignment> =
            d.assignments.iter().filter(|a| !d.groups.iter().any(|g| Some(g.id) == a.assignment_group_id)).collect();
        if !loose.is_empty() {
            groups.push(("Other".into(), None, loose));
        }
        if groups.is_empty() {
            empty(ui, "No graded work yet.");
            return;
        }

        // Summary strip: weighting scheme and each group's share.
        ui.add_space(18.0);
        ui.label(txt(if d.weighted { "Weighted by group" } else { "Total points" }, 12.5, p.text3));
        for (name, weight, list) in &groups {
            if list.is_empty() {
                continue;
            }
            let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::hover());
            ui.painter().text(
                pos2(rect.left(), rect.center().y),
                Align2::LEFT_CENTER,
                name,
                FontId::new(13.0, theme::medium()),
                p.text,
            );
            let right = match (pct(list), hide_total) {
                (Some(v), false) => format!("{v:.1}%"),
                (None, _) => "–".into(),
                (_, true) => "••".into(),
            };
            ui.painter().text(
                pos2(rect.right(), rect.center().y),
                Align2::RIGHT_CENTER,
                right,
                FontId::monospace(12.5),
                p.text,
            );
            if d.weighted
                && let Some(w) = weight
            {
                ui.painter().text(
                    pos2(rect.right() - 90.0, rect.center().y),
                    Align2::RIGHT_CENTER,
                    format!("{w:.0}% of grade"),
                    FontId::monospace(12.0),
                    p.text3,
                );
            }
            ui.painter().hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0, p.border));
        }

        for (name, _, list) in groups {
            if list.is_empty() {
                continue;
            }
            section(ui, &name, list.len());
            let mut list = list;
            list.sort_by_key(|a| a.due_at);
            for a in list {
                assignment_row(ui, out, a, hide_scores);
            }
        }
    }

    fn syllabus_tab(&self, ui: &mut Ui, out: &mut Vec<Action>, id: u64, d: &CourseDetail, filter: &mut String) {
        let p = pal(ui);
        let scraped = self.data.syllabi.get(&id);
        let body = d.syllabus.as_ref().filter(|s| s.len() >= 400 || d.syllabus_tool.is_none());
        let (text, source, url) = match (scraped, body) {
            (_, Some(b)) => {
                (Some(b.as_str()), "Canvas".to_string(), Some(format!("/courses/{id}/assignments/syllabus")))
            }
            (Some(s), None) => (Some(s.text.as_str()), s.source.clone(), s.url.clone()),
            (None, None) => (None, String::new(), None),
        };

        if let Some(text) = text {
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.label(txt(format!("Source  ·  {source}"), 12.5, p.text3));
                if let Some(u) = &url
                    && theme::text_button(ui, "Open original", p.text2).clicked()
                {
                    out.push(Action::Open(u.clone()));
                }
                if scraped.is_some() && self.syllabus_pending(id) {
                    theme::spinner(ui, 11.0);
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    theme::input(ui, filter, "Find in syllabus", 240.0, false);
                });
            });
            ui.add_space(12.0);
            syllabus_text(ui, text, filter);
        } else if d.syllabus_tool.is_some() && (self.syllabus_pending(id) || !self.syllabus_errors.contains_key(&id)) {
            let label = d.syllabus_tool.as_ref().map(|t| t.0.as_str()).unwrap_or("the syllabus tool");
            loading(ui, &format!("Fetching syllabus from {label}. The first load takes about 15 seconds"));
        } else {
            ui.add_space(16.0);
            match self.syllabus_errors.get(&id) {
                Some(e) => ui.label(txt(e, 13.5, p.danger)),
                None => ui.label(txt("This course has no syllabus in Canvas.", 13.5, p.text3)),
            };
            if let Some((label, path)) = &d.syllabus_tool {
                ui.add_space(10.0);
                if theme::button(ui, &format!("Open {label}"), false).clicked() {
                    out.push(Action::Open(path.clone()));
                }
            }
        }

        let module_links: Vec<(String, String)> = d
            .modules
            .iter()
            .flat_map(|m| &m.items)
            .filter(|it| it.title.to_lowercase().contains("syllabus"))
            .filter_map(|it| Some((it.title.clone(), it.html_url.clone()?)))
            .collect();
        let links: Vec<&(String, String)> = d
            .syllabus_links
            .iter()
            .filter(|(_, h)| !h.contains("/external_tools/"))
            .chain(module_links.iter())
            .collect();
        if !links.is_empty() {
            ui.spacing_mut().item_spacing.y = 0.0;
            section(ui, "Files & pages", links.len());
            for (label, href) in links {
                let row = Row::plain(egui::Id::unique(("syl-link", href.as_str())), label, Col::empty());
                if row.show(ui, out).title_clicked {
                    out.push(Action::Open(href.clone()));
                }
            }
        }
    }

    // ---------- settings ----------

    /// Signed-out screen: a single centred column.
    fn sign_in(&mut self, ui: &mut Ui) {
        let p = pal(ui);
        drag_region(ui, 38.0);
        let top = (ui.available_height() * 0.16).max(24.0);
        ui.add_space(top);
        ui.vertical_centered(|ui| {
            ui.set_max_width(360.0);
            let (r, _) = ui.allocate_exact_size(vec2(36.0, 36.0), Sense::hover());
            logo(ui, r);
            ui.add_space(22.0);
            ui.label(semi("Sign in to Canvas", 22.0, p.text));
            ui.add_space(6.0);
            ui.label(txt("Use your school's normal login. Nothing leaves this Mac.", 13.5, p.text3));
            ui.add_space(26.0);
            ui.with_layout(Layout::top_down(Align::Min), |ui| {
                ui.label(med("Canvas URL", 12.5, p.text2));
                ui.add_space(6.0);
                let r = theme::input(ui, &mut self.form_url, "canvas.yourschool.edu", ui.available_width(), false);
                if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    self.browser_login();
                }
                ui.add_space(12.0);
                let busy = self.inflight > 0;
                if theme::wide_button(ui, if busy { "Waiting for sign-in…" } else { "Continue with browser" }, !busy)
                    .clicked()
                    && !busy
                {
                    self.browser_login();
                }
                ui.add_space(22.0);
                theme::hairline(ui);
                ui.add_space(14.0);
                let id = egui::Id::unique("show-token");
                let mut show = ui.data(|d| d.get_temp::<bool>(id).unwrap_or(false));
                if theme::text_button(
                    ui,
                    if show { "Hide access token" } else { "Use an access token instead" },
                    p.text3,
                )
                .clicked()
                {
                    show = !show;
                    ui.data_mut(|d| d.insert_temp(id, show));
                }
                if show {
                    ui.add_space(10.0);
                    ui.label(txt("Account → Settings → Approved Integrations → New Access Token", 12.0, p.text3));
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        let r =
                            theme::input(ui, &mut self.form_token, "Access token", ui.available_width() - 92.0, true);
                        if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                            self.connect();
                        }
                        if theme::button(ui, "Connect", false).clicked() {
                            self.connect();
                        }
                    });
                }
                if let Some((ok, m)) = &self.form_msg {
                    ui.add_space(14.0);
                    ui.label(txt(m, 13.0, if *ok { p.text2 } else { p.danger }));
                }
            });
        });
    }

    fn settings(&mut self, ui: &mut Ui) {
        let p = pal(ui);
        header_bar(ui, &[("Settings", true)], None, |_| {});
        let mut sign_out = false;
        ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            Frame::new().inner_margin(Margin { left: PAD as i8, right: PAD as i8, top: 28, bottom: 40 }).show(
                ui,
                |ui| {
                    ui.set_max_width(ui.available_width().min(720.0));

                    let method = if self.api.as_ref().is_some_and(|a| a.uses_session()) {
                        "Browser session"
                    } else {
                        "Access token"
                    };
                    card(ui, "Account", |ui| {
                        let name = self.data.user_name.clone().unwrap_or_default();
                        let school = self.base_url.trim_start_matches("https://").to_string();
                        for (i, (k, v)) in [("Name", name), ("School", school), ("Signed in with", method.to_string())]
                            .into_iter()
                            .enumerate()
                        {
                            if i > 0 {
                                theme::hairline(ui);
                            }
                            setting_row(ui, k, None, |ui| {
                                ui.label(txt(v, size::BASE, p.text2));
                            });
                        }
                        theme::hairline(ui);
                        setting_row(
                            ui,
                            "Sign out",
                            Some("Removes your saved login and cached data from this Mac."),
                            |ui| {
                                if theme::button(ui, "Sign out", false).clicked() {
                                    sign_out = true;
                                }
                            },
                        );
                    });

                    card(ui, "Appearance", |ui| {
                        setting_row(ui, "Theme", Some("Match your Mac, or always use light or dark."), |ui| {
                            let modes = ["system", "light", "dark"];
                            let cur = modes.iter().position(|m| *m == self.prefs.appearance).unwrap_or(0);
                            if let Some(i) = theme::segmented(ui, &["System", "Light", "Dark"], cur) {
                                self.set_appearance(modes[i]);
                            }
                        });
                    });

                    let (mut hide_grades, mut hide_scores) = (self.prefs.hide_grades, self.prefs.hide_scores);
                    card(ui, "Privacy", |ui| {
                        setting_row(
                            ui,
                            "Hide course grades",
                            Some("Hidden in the sidebar and course headers. Show grade reveals one course."),
                            |ui| {
                                theme::toggle(ui, &mut hide_grades, "");
                            },
                        );
                        theme::hairline(ui);
                        setting_row(
                            ui,
                            "Hide assignment scores",
                            Some("Scores on assignments and in the Grades tab."),
                            |ui| {
                                theme::toggle(ui, &mut hide_scores, "");
                            },
                        );
                    });
                    if hide_grades != self.prefs.hide_grades || hide_scores != self.prefs.hide_scores {
                        self.prefs.hide_grades = hide_grades;
                        self.prefs.hide_scores = hide_scores;
                        self.revealed.clear();
                        store::save_prefs(&self.prefs);
                    }

                    let mut mb = self.prefs.menu_bar;
                    let modes = ["none", "today", "week", "next"];
                    let mut show = modes.iter().position(|m| *m == self.prefs.menu_bar_show).unwrap_or(1);
                    card(ui, "Menu bar", |ui| {
                        setting_row(
                            ui,
                            "Show in menu bar",
                            Some("One click lists what's due next. Closing the window keeps it running there."),
                            |ui| {
                                theme::toggle(ui, &mut mb, "");
                            },
                        );
                        theme::hairline(ui);
                        setting_row(
                            ui,
                            "Beside the icon",
                            Some("A count of what's left, or the next item's name."),
                            |ui| {
                                ui.add_enabled_ui(mb, |ui| {
                                    if let Some(i) =
                                        theme::segmented(ui, &["Nothing", "Today", "This week", "Next item"], show)
                                    {
                                        show = i;
                                    }
                                });
                            },
                        );
                    });
                    if mb != self.prefs.menu_bar || modes[show] != self.prefs.menu_bar_show {
                        self.set_menu_bar(mb, modes[show]);
                    }

                    card(ui, "Keyboard", |ui| {
                        let keys: [(&[&str], &str); 7] = [
                            (&["U"], "Upcoming"),
                            (&["C"], "Calendar"),
                            (&["W", "M"], "Calendar week or month"),
                            (&["←", "→"], "Previous or next week or month"),
                            (&["T"], "Jump to today"),
                            (&["1", "9"], "Open a course"),
                            (&["R"], "Refresh"),
                        ];
                        for (i, (k, label)) in keys.iter().enumerate() {
                            if i > 0 {
                                theme::hairline(ui);
                            }
                            setting_row(ui, label, None, |ui| {
                                for (j, key) in k.iter().rev().enumerate() {
                                    if j > 0 {
                                        ui.label(txt(
                                            if *label == "Open a course" { "to" } else { "or" },
                                            size::SM,
                                            p.text3,
                                        ));
                                    }
                                    theme::kbd(ui, key);
                                }
                            });
                        }
                    });
                    ui.add_space(4.0);
                    ui.label(txt(
                        "Data is cached on this Mac so the app opens instantly, and refreshes every 10 minutes.",
                        size::SM,
                        p.text3,
                    ));
                },
            );
        });
        if sign_out {
            store::clear_config(&self.base_url);
            self.api = None;
            self.data = Cache::default();
            self.form_msg = Some((true, "Signed out. Saved login and cache deleted.".into()));
        }
    }
}

// ---------- shared pieces ----------

/// Shown above every page once the browser session expired and couldn't renew itself.
fn session_banner(ui: &mut Ui, out: &mut Vec<Action>) {
    let p = pal(ui);
    Frame::new().fill(p.raised).inner_margin(Margin::symmetric(PAD as i8, 10)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(med("Your Canvas session expired.", size::BASE, p.text));
            ui.label(txt("What you see may be out of date.", size::BASE, p.text3));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if theme::button(ui, "Sign in again", true).clicked() {
                    out.push(Action::Reauth);
                }
            });
        });
    });
    theme::hairline(ui);
}

/// Top bar of every page: breadcrumb on the left, controls on the right, hairline below.
/// `crumbs` are (text, is_current).
pub(super) fn header_bar(ui: &mut Ui, crumbs: &[(&str, bool)], note: Option<&str>, right: impl FnOnce(&mut Ui)) {
    let p = pal(ui);
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), HEADER_H), Sense::click_and_drag());
    if resp.drag_started() {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
    let mut x = rect.left() + PAD;
    let y = rect.center().y;
    for (i, (c, current)) in crumbs.iter().enumerate() {
        if i > 0 {
            let g = ui.painter().layout_no_wrap("/".into(), FontId::proportional(13.5), p.text4);
            ui.painter().galley(pos2(x + 8.0, y - g.size().y / 2.0), g.clone(), p.text4);
            x += g.size().x + 16.0;
        }
        let font = FontId::new(13.5, theme::medium());
        let g = ui.painter().layout_no_wrap(c.to_string(), font, if *current { p.text } else { p.text3 });
        ui.painter().galley(pos2(x, y - g.size().y / 2.0), g.clone(), p.text);
        x += g.size().x;
    }
    if let Some(n) = note {
        ui.painter().text(pos2(x + 12.0, y), Align2::LEFT_CENTER, n, FontId::proportional(size::SM), p.text3);
    }
    let controls = Rect::from_min_max(pos2(rect.center().x, rect.top()), pos2(rect.right() - PAD + 6.0, rect.bottom()));
    ui.scope_builder(egui::UiBuilder::new().max_rect(controls).layout(Layout::right_to_left(Align::Center)), |ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        right(ui);
    });
    ui.painter().hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0, p.border));
    // Content starts flush under the rule.
    ui.add_space(-ui.spacing().item_spacing.y);
}

fn drag_region(ui: &mut Ui, h: f32) {
    let (_, resp) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::click_and_drag());
    if resp.drag_started() {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
}

/// The mark: a solid square with a cut corner — a blank canvas, folded.
fn logo(ui: &Ui, r: Rect) {
    let p = pal(ui);
    let s = r.width();
    let pts = vec![
        r.left_top(),
        pos2(r.right() - s * 0.32, r.top()),
        pos2(r.right(), r.top() + s * 0.32),
        r.right_bottom(),
        r.left_bottom(),
    ];
    ui.painter().add(egui::Shape::convex_polygon(pts, p.text, Stroke::NONE));
    let fold = vec![
        pos2(r.right() - s * 0.32, r.top()),
        pos2(r.right() - s * 0.32, r.top() + s * 0.32),
        pos2(r.right(), r.top() + s * 0.32),
    ];
    ui.painter().add(egui::Shape::convex_polygon(fold, p.text3, Stroke::NONE));
}

fn nav_item(
    ui: &mut Ui,
    icon: Option<Icon>,
    square: Option<Color32>,
    label: &str,
    right: Option<&str>,
    selected: bool,
) -> egui::Response {
    let p = pal(ui);
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click());
    if selected {
        ui.painter().rect_filled(rect, 6.0, p.hover);
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, 6.0, p.hover.gamma_multiply(0.7));
    }
    let fg = if selected || resp.hovered() { p.text } else { p.text2 };
    let mut x = rect.left() + 8.0;
    if let Some(i) = icon {
        theme::paint_icon(ui, Rect::from_center_size(pos2(x + 7.0, rect.center().y), vec2(16.0, 16.0)), i, fg);
        x += 24.0;
    } else if let Some(c) = square {
        theme::course_square(ui, pos2(x + 5.0, rect.center().y), c, 7.0);
        x += 20.0;
    }
    let right_w = right.map(|_| 44.0).unwrap_or(0.0);
    let g = theme::galley(ui, label, FontId::new(13.0, theme::medium()), fg, rect.right() - x - right_w - 8.0);
    ui.painter().galley(pos2(x, rect.center().y - g.size().y / 2.0), g, fg);
    if let Some(r) = right {
        ui.painter().text(
            pos2(rect.right() - 8.0, rect.center().y),
            Align2::RIGHT_CENTER,
            r,
            FontId::monospace(11.5),
            p.text3,
        );
    }
    theme::a11y(&resp, egui::Role::Button, label, Some(selected));
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Group header for a list: label, quiet secondary text, and a mono count on the right.
fn group_header(ui: &mut Ui, label: &str, sub: &str, count: &str, quiet: bool, color: Option<Color32>) {
    let p = pal(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, p.raised);
    ui.painter().hline(rect.x_range(), rect.top() + 0.5, Stroke::new(1.0, p.border));
    ui.painter().hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0, p.border));
    let fg = color.unwrap_or(if quiet { p.text3 } else { p.text });
    let g = ui.painter().layout_no_wrap(label.to_string(), FontId::new(13.0, theme::medium()), fg);
    let w = g.size().x;
    ui.painter().galley(pos2(rect.left() + PAD, rect.center().y - g.size().y / 2.0), g, fg);
    if !sub.is_empty() {
        ui.painter().text(
            pos2(rect.left() + PAD + w + 10.0, rect.center().y),
            Align2::LEFT_CENTER,
            sub,
            FontId::proportional(12.5),
            p.text3,
        );
    }
    if !count.is_empty() {
        ui.painter().text(
            pos2(rect.right() - PAD, rect.center().y),
            Align2::RIGHT_CENTER,
            count,
            FontId::monospace(12.0),
            p.text3,
        );
    }
}

fn section(ui: &mut Ui, title: &str, count: usize) {
    let p = pal(ui);
    ui.add_space(26.0);
    ui.horizontal(|ui| {
        ui.label(med(title, 12.5, p.text2));
        if count > 0 {
            ui.label(mono(count.to_string(), 12.0, p.text4));
        }
    });
    ui.add_space(8.0);
    theme::hairline(ui);
}

fn empty(ui: &mut Ui, msg: &str) {
    let p = pal(ui);
    ui.add_space(14.0);
    ui.label(txt(msg, 13.5, p.text3));
    ui.add_space(6.0);
}

pub(super) fn loading(ui: &mut Ui, msg: &str) {
    let p = pal(ui);
    ui.add_space(20.0);
    ui.horizontal(|ui| {
        ui.add_space(PAD);
        theme::spinner(ui, 12.0);
        ui.label(txt(msg, 13.0, p.text3));
    });
}

/// A bordered settings card with a title bar.
fn card(ui: &mut Ui, title: &str, add: impl FnOnce(&mut Ui)) {
    let p = pal(ui);
    Frame::new().stroke(Stroke::new(1.0, p.border)).corner_radius(8).fill(p.bg).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.set_width(ui.available_width());
        Frame::new()
            .fill(p.raised)
            .corner_radius(egui::CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 })
            .inner_margin(Margin::symmetric(16, 12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(med(title, size::BASE, p.text));
            });
        theme::hairline(ui);
        Frame::new().inner_margin(Margin::symmetric(16, 0)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui);
        });
    });
    ui.add_space(20.0);
}

/// Label (+ optional one-line description) on the left, control on the right.
fn setting_row(ui: &mut Ui, label: &str, desc: Option<&str>, control: impl FnOnce(&mut Ui)) {
    let p = pal(ui);
    let h = if desc.is_some() { 60.0 } else { 44.0 };
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::hover());
    let text_w = rect.width() * 0.72;
    if let Some(d) = desc {
        ui.painter().text(
            pos2(rect.left(), rect.center().y - 9.0),
            Align2::LEFT_CENTER,
            label,
            FontId::new(size::BASE, theme::medium()),
            p.text,
        );
        let g = theme::galley(ui, d, FontId::proportional(size::SM), p.text3, text_w);
        ui.painter().galley(pos2(rect.left(), rect.center().y + 2.0), g, p.text3);
    } else {
        ui.painter().text(
            pos2(rect.left(), rect.center().y),
            Align2::LEFT_CENTER,
            label,
            FontId::proportional(size::BASE),
            p.text3,
        );
    }
    let right = Rect::from_min_max(pos2(rect.left() + text_w, rect.top()), rect.max);
    ui.scope_builder(egui::UiBuilder::new().max_rect(right).layout(Layout::right_to_left(Align::Center)), |ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        control(ui);
    });
}

/// A right-aligned column cell in a list row.
pub(super) struct Col<'a> {
    text: &'a str,
    color: Color32,
    mono: bool,
}

impl<'a> Col<'a> {
    fn text(text: &'a str, color: Color32) -> Self {
        Self { text, color, mono: false }
    }
    fn mono(text: &'a str, color: Color32) -> Self {
        Self { text, color, mono: true }
    }
    fn empty() -> Self {
        Self { text: "", color: Color32::TRANSPARENT, mono: false }
    }
}

/// One list row: status circle, title, course label, and three fixed-width
/// right columns so values line up down the list.
struct Row<'a> {
    id: egui::Id,
    status: Option<(Status, bool)>,
    title: &'a str,
    dim: bool,
    course: Option<(String, Color32, Option<u64>)>,
    cols: [Col<'a>; 3],
    /// Widths of the three right-hand columns.
    widths: [f32; 3],
}

const COLS: [f32; 3] = [84.0, 64.0, 76.0];

#[derive(Default)]
struct RowResult {
    status_clicked: bool,
    title_clicked: bool,
}

impl<'a> Row<'a> {
    fn plain(id: egui::Id, title: &'a str, last: Col<'a>) -> Self {
        Self {
            id,
            status: None,
            title,
            dim: false,
            course: None,
            cols: [Col::empty(), Col::empty(), last],
            widths: COLS,
        }
    }

    fn show(self, ui: &mut Ui, out: &mut Vec<Action>) -> RowResult {
        let p = pal(ui);
        let mut res = RowResult::default();
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::hover());
        if resp.hovered() {
            ui.painter().rect_filled(rect, 0.0, p.hover);
        }
        let y = rect.center().y;
        let mut x = rect.left() + PAD;
        if let Some((status, enabled)) = self.status {
            if theme::status_circle(ui, pos2(x + 7.0, y), status, enabled, self.id.with("status")).clicked() {
                res.status_clicked = true;
            }
            x += 28.0;
        }

        // Right columns, laid out from the right edge: [c0 76][c1 64][c2 76].
        let widths = self.widths;
        let mut rx = rect.right() - PAD;
        for (col, w) in self.cols.iter().zip(widths).rev() {
            if !col.text.is_empty() {
                let font = if col.mono { FontId::monospace(12.0) } else { FontId::proportional(size::SM) };
                let color = if self.dim { p.text3 } else { col.color };
                ui.painter().text(pos2(rx, y), Align2::RIGHT_CENTER, col.text, font, color);
            }
            rx -= w;
        }

        let course_w = self
            .course
            .as_ref()
            .map(|(l, ..)| if l.is_empty() { 0.0 } else { 18.0 + l.len() as f32 * 6.8 })
            .unwrap_or(0.0);
        let max_title = (rx - x - course_w - 20.0).max(60.0);
        let fg = if self.dim { p.text3 } else { p.text };
        let title = crate::names::clean_title(self.title);
        let g = theme::galley(ui, &title, FontId::proportional(size::BASE), fg, max_title);
        let title_rect = Rect::from_min_size(pos2(x, y - g.size().y / 2.0), g.size());
        let tr = ui.interact(title_rect.expand(3.0), self.id.with("title"), Sense::click());
        theme::a11y(&tr, egui::Role::Link, self.title, None);
        let tfg = if tr.hovered() && !self.dim { p.text } else { fg };
        ui.painter().galley(title_rect.min, g.clone(), tfg);
        if tr.hovered() {
            ui.painter().hline(title_rect.x_range(), title_rect.bottom() + 1.0, Stroke::new(1.0, p.text3));
        }
        if tr.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            res.title_clicked = true;
        }

        if let Some((label, color, cid)) = self.course.filter(|c| !c.0.is_empty()) {
            let cx = title_rect.right() + 14.0;
            theme::course_square(ui, pos2(cx + 3.0, y), color, 6.0);
            let label_text = label.clone();
            let lg = ui.painter().layout_no_wrap(label, FontId::proportional(size::SM), p.text3);
            let lr = Rect::from_min_size(pos2(cx + 12.0, y - lg.size().y / 2.0), lg.size());
            let cr = ui.interact(lr.expand(3.0), self.id.with("course"), Sense::click());
            theme::a11y(&cr, egui::Role::Link, &label_text, None);
            ui.painter().galley(lr.min, lg, if cr.hovered() { p.text } else { p.text3 });
            if cr.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
                && let Some(id) = cid
            {
                out.push(Action::View(View::Course(id)));
            }
        }
        ui.painter().hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0, p.border));
        res
    }
}

fn item_status<'a>(it: &PlannerItem, p: &Pal) -> (&'a str, Color32) {
    if it.graded() {
        ("Graded", p.text3)
    } else if it.submitted() {
        ("Submitted", p.text3)
    } else if it.missing() {
        ("Missing", p.danger)
    } else if it.late() {
        ("Late", p.text2)
    } else {
        ("", p.text3)
    }
}

fn assignment_row(ui: &mut Ui, out: &mut Vec<Action>, a: &Assignment, hide_scores: bool) {
    let p = pal(ui);
    let sub = a.submission.as_ref();
    let due = a.due_at.map(|d| d.with_timezone(&Local).format("%a %b %-d, %-I:%M %p").to_string()).unwrap_or_default();
    let score = match (sub.and_then(|s| s.score).filter(|_| !hide_scores), a.points_possible) {
        (Some(s), Some(pp)) if pp > 0.0 => format!("{}/{}", fmt_num(s), fmt_num(pp)),
        (Some(s), _) => fmt_num(s),
        (None, Some(pp)) if pp > 0.0 => format!("{} pts", fmt_num(pp)),
        _ => String::new(),
    };
    let scored = sub.and_then(|s| s.score).is_some();
    let (word, wc) = match sub {
        Some(s) if s.excused == Some(true) => ("Excused", p.text3),
        Some(s) if s.missing => ("Missing", p.danger),
        Some(s) if s.late => ("Late", p.text2),
        Some(_) if submitted(a) && !scored => ("Submitted", p.text3),
        _ => ("", p.text3),
    };
    let status = if sub.is_some_and(|s| s.missing) {
        Status::Missing
    } else if submitted(a) || scored {
        Status::Done
    } else {
        Status::Todo
    };
    let row = Row {
        id: egui::Id::unique(("asg", a.id)),
        status: Some((status, false)),
        title: &a.name,
        dim: false,
        course: None,
        widths: [84.0, 76.0, 160.0],
        cols: [Col::text(word, wc), Col::mono(&score, if scored { p.text } else { p.text3 }), Col::mono(&due, p.text2)],
    };
    if row.show(ui, out).title_clicked
        && let Some(u) = &a.html_url
    {
        out.push(Action::Open(u.clone()));
    }
}

fn assignments(ui: &mut Ui, out: &mut Vec<Action>, d: &CourseDetail, hide_scores: bool) {
    let now = Utc::now();
    let (mut up, mut past, mut undated) = (vec![], vec![], vec![]);
    for a in &d.assignments {
        match a.due_at {
            Some(t) if t >= now => up.push(a),
            Some(_) => past.push(a),
            None => undated.push(a),
        }
    }
    past.reverse();
    ui.spacing_mut().item_spacing.y = 0.0;
    for (name, list) in [("Upcoming", up), ("No due date", undated), ("Past", past)] {
        if list.is_empty() {
            continue;
        }
        section(ui, name, list.len());
        for a in list {
            assignment_row(ui, out, a, hide_scores);
        }
    }
    if d.assignments.is_empty() {
        empty(ui, "No assignments.");
    }
}

fn announcement(ui: &mut Ui, out: &mut Vec<Action>, a: &Announcement, preview: Option<usize>) {
    let p = pal(ui);
    ui.add_space(16.0);
    ui.horizontal(|ui| {
        let r = ui.add(egui::Label::new(med(&a.title, size::MD, p.text)).sense(Sense::click()).truncate());
        if r.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
            && let Some(u) = &a.html_url
        {
            out.push(Action::Open(u.clone()));
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if let Some(t) = a.posted_at {
                ui.label(mono(t.with_timezone(&Local).format("%b %-d").to_string(), 12.0, p.text3));
            }
            if let Some(n) = a.author.as_ref().and_then(|a| a.display_name.as_deref()) {
                ui.label(txt(n, 12.5, p.text3));
            }
        });
    });
    if let Some(m) = &a.message {
        let body = match preview {
            Some(n) if m.chars().count() > n => format!("{}…", m.chars().take(n).collect::<String>().trim_end()),
            _ => m.clone(),
        };
        let body = if preview.is_some() { body.replace("\n\n", "\n") } else { body };
        ui.add_space(6.0);
        ui.add(egui::Label::new(txt(body, 13.5, p.text2).line_height(Some(20.0))).selectable(true));
    }
    ui.add_space(16.0);
    theme::hairline(ui);
}

fn modules(ui: &mut Ui, out: &mut Vec<Action>, d: &CourseDetail) {
    let p = pal(ui);
    if d.modules.is_empty() {
        empty(ui, "No modules, or they're hidden for this course.");
    }
    ui.spacing_mut().item_spacing.y = 0.0;
    ui.add_space(8.0);
    for m in &d.modules {
        module_block(ui, out, m, &p);
    }
}

fn module_block(ui: &mut Ui, out: &mut Vec<Action>, m: &Module, p: &Pal) {
    let id = egui::Id::unique(("module", m.id));
    let mut open = ui.data(|d| d.get_temp::<bool>(id).unwrap_or(false));
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::click());
    theme::a11y(&resp, egui::Role::Button, &m.name, Some(open));
    if resp.hovered() {
        ui.painter().rect_filled(rect, 0.0, p.hover);
    }
    let icon_r = Rect::from_center_size(pos2(rect.left() + 10.0, rect.center().y), vec2(14.0, 14.0));
    theme::paint_icon(ui, icon_r, if open { Icon::ChevronDown } else { Icon::ChevronRight }, p.text3);
    let items = m.items.iter().filter(|i| i.kind != "SubHeader").count();
    let g = theme::galley(ui, &m.name, FontId::new(13.5, theme::medium()), p.text, rect.width() - 90.0);
    ui.painter().galley(pos2(rect.left() + 26.0, rect.center().y - g.size().y / 2.0), g, p.text);
    ui.painter().text(
        pos2(rect.right(), rect.center().y),
        Align2::RIGHT_CENTER,
        items.to_string(),
        FontId::monospace(12.0),
        p.text4,
    );
    ui.painter().hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0, p.border));
    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
        open = !open;
        ui.data_mut(|d| d.insert_temp(id, open));
    }
    if !open {
        return;
    }
    for it in &m.items {
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::click());
        let x = rect.left() + 26.0 + it.indent as f32 * 16.0;
        if it.kind == "SubHeader" {
            ui.painter().text(
                pos2(x, rect.center().y + 3.0),
                Align2::LEFT_CENTER,
                &it.title,
                FontId::new(12.0, theme::medium()),
                p.text3,
            );
            continue;
        }
        theme::a11y(&resp, egui::Role::Link, &it.title, None);
        if resp.hovered() {
            ui.painter().rect_filled(rect, 0.0, p.hover);
        }
        ui.painter().text(
            pos2(x, rect.center().y),
            Align2::LEFT_CENTER,
            module_kind(&it.kind),
            FontId::monospace(11.0),
            p.text4,
        );
        let fg = if resp.hovered() { p.text } else { p.text2 };
        let g = theme::galley(ui, &it.title, FontId::proportional(13.5), fg, rect.right() - x - 60.0);
        ui.painter().galley(pos2(x + 48.0, rect.center().y - g.size().y / 2.0), g, fg);
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
            && let Some(u) = &it.html_url
        {
            out.push(Action::Open(u.clone()));
        }
    }
    ui.add_space(6.0);
}

/// Renders syllabus text: short standalone lines become headings, and an
/// optional filter keeps only matching lines (under their heading).
fn syllabus_text(ui: &mut Ui, text: &str, filter: &str) {
    let p = pal(ui);
    let lines: Vec<&str> = text.lines().collect();
    let is_heading = |i: usize| {
        let l = lines[i].trim();
        let blank = |j: Option<usize>| j.and_then(|j| lines.get(j)).is_none_or(|l| l.trim().is_empty());
        !l.is_empty()
            && l.len() <= 48
            && l.split_whitespace().count() <= 6
            && !l.ends_with(['.', ':', ','])
            && l.chars().next().is_some_and(char::is_uppercase)
            && blank(i.checked_sub(1))
            && blank(Some(i + 1))
    };
    let needle = filter.trim().to_lowercase();
    let mut heading_shown: Option<usize> = None;
    let mut last_heading: Option<usize> = None;
    let mut matches = 0;
    ui.spacing_mut().item_spacing.y = 3.0;
    for (i, line) in lines.iter().enumerate() {
        if is_heading(i) {
            last_heading = Some(i);
            if needle.is_empty() {
                ui.add_space(18.0);
                ui.label(semi(line.trim(), size::MD, p.text));
                ui.add_space(4.0);
            }
            continue;
        }
        if line.trim().is_empty() {
            if needle.is_empty() {
                ui.add_space(6.0);
            }
            continue;
        }
        if !needle.is_empty() {
            if !line.to_lowercase().contains(&needle) {
                continue;
            }
            matches += 1;
            if last_heading != heading_shown
                && let Some(h) = last_heading
            {
                ui.add_space(14.0);
                ui.label(med(lines[h].trim(), 13.5, p.text));
                heading_shown = last_heading;
            }
        }
        ui.add(egui::Label::new(txt(*line, 13.5, p.text2).line_height(Some(20.0))).selectable(true));
    }
    if !needle.is_empty() && matches == 0 {
        empty(ui, "No matches.");
    }
}

fn module_kind(k: &str) -> &'static str {
    match k {
        "Assignment" => "TASK",
        "Quiz" => "QUIZ",
        "Discussion" => "DISC",
        "File" => "FILE",
        "Page" => "PAGE",
        "ExternalUrl" | "ExternalTool" => "LINK",
        _ => "",
    }
}

pub(super) fn kind_label(t: &str) -> Option<&'static str> {
    match t {
        "quiz" => Some("Quiz"),
        "discussion_topic" => Some("Discussion"),
        "wiki_page" => Some("Page"),
        "calendar_event" => Some("Event"),
        "planner_note" => Some("To-do"),
        "assessment_request" => Some("Peer review"),
        _ => None,
    }
}
