use crate::api::{Api, Assignment, Course, CourseDetail, PlannerItem, PlannerOverride, User};
use crate::store::{self, Cache, Config, Syllabus};
use crate::tray::{Entry, Tray, TrayAction};
use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, NaiveTime, TimeZone, Utc};
use eframe::egui::{self, Color32, Key};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tray_icon::menu::MenuEvent;

mod calendar;
mod ui;

const AUTO_REFRESH_SECS: u64 = 10 * 60;
/// Scraped syllabi rarely change; re-scrape after this long.
const SYLLABUS_MAX_AGE_DAYS: i64 = 3;

type Res<T> = Result<T, String>;

enum Msg {
    Connected(Res<(Config, User)>),
    Me(Res<User>),
    Courses(Res<Vec<Course>>),
    Colors(Res<HashMap<u64, String>>),
    Planner(String, Res<Vec<PlannerItem>>),
    Missing(Res<Vec<Assignment>>),
    Detail(u64, Res<CourseDetail>),
    Override { week: String, plannable_id: u64, res: Res<PlannerOverride> },
    Syllabus(u64, Res<Syllabus>),
}

#[derive(Clone, Copy, PartialEq)]
enum View {
    Upcoming,
    Calendar,
    Course(u64),
    Settings,
}

#[derive(Clone, Copy, PartialEq)]
enum CalMode {
    Week,
    Month,
}

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Overview,
    Grades,
    Assignments,
    Announcements,
    Modules,
    Syllabus,
}

/// An exam from Canvas (an assignment titled like one), the syllabus, or both.
#[derive(Clone)]
struct Exam {
    course_id: u64,
    label: String,
    date: NaiveDate,
    time: Option<NaiveTime>,
    /// Set when a Canvas assignment backs this exam (it then shows up as a planner item too).
    canvas_id: Option<u64>,
    url: Option<String>,
    from_syllabus: bool,
    /// Only the week is known; `date` is the week's first day.
    approx: bool,
}

enum Action {
    Open(String),
    View(View),
    Tab(Tab),
    ShiftWeek(i64),
    /// Move the calendar by one week or month (sign gives direction).
    CalShift(i64),
    CalMode(CalMode),
    /// Jump the calendar to a date in the given mode.
    CalGoto(NaiveDate, CalMode),
    Toggle {
        week: String,
        plannable_id: u64,
    },
    /// Open a course on a specific tab.
    CourseTab(u64, Tab),
    RevealGrade(u64),
    Refresh,
}

pub struct App {
    ctx: egui::Context,
    api: Option<Arc<Api>>,
    base_url: String,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    data: Cache,
    view: View,
    tab: Tab,
    week_offset: i64,
    cal_mode: CalMode,
    /// Any date inside the calendar's visible week or month.
    cal_anchor: NaiveDate,
    hide_done: bool,
    inflight: usize,
    error: Option<String>,
    /// Things already refreshed this session (cache may still show older data).
    fresh: HashSet<String>,
    toggling: HashSet<u64>,
    last_refresh: Option<Instant>,
    dirty: bool,
    last_save: Instant,
    form_url: String,
    form_token: String,
    form_msg: Option<(bool, String)>,
    /// Courses waiting for their syllabus tool to be scraped: (course, label, Canvas path).
    syllabus_queue: Arc<Mutex<VecDeque<(u64, String, String)>>>,
    syllabus_worker: Arc<AtomicBool>,
    syllabus_errors: HashMap<u64, String>,
    syllabus_filter: String,
    prefs: store::Prefs,
    exams: Vec<Exam>,
    exams_stale: bool,
    /// Courses whose hidden grade was revealed for this session.
    revealed: HashSet<u64>,
    tray: Option<Tray>,
    tray_rx: Receiver<MenuEvent>,
    tray_dirty: bool,
    tray_built: Instant,
    /// Window closed while the menu bar item keeps the app alive.
    hidden: bool,
    /// Since hiding, the app has been seen in the background (so a later activation is a reopen).
    hidden_went_inactive: bool,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        crate::theme::install(&cc.egui_ctx);
        let (tx, rx) = channel();
        let cfg = store::load_config();
        let api = cfg.as_ref().and_then(|c| Api::new(c).ok()).map(Arc::new);
        let mut app = Self {
            ctx: cc.egui_ctx.clone(),
            view: if api.is_some() { View::Upcoming } else { View::Settings },
            api,
            tx,
            rx,
            data: cfg.as_ref().map(|c| store::load_cache(&c.base_url)).unwrap_or_default(),
            base_url: cfg.as_ref().map(|c| c.base_url.clone()).unwrap_or_default(),
            tab: Tab::Overview,
            week_offset: 0,
            cal_mode: CalMode::Week,
            cal_anchor: Local::now().date_naive(),
            hide_done: false,
            inflight: 0,
            error: None,
            fresh: HashSet::new(),
            toggling: HashSet::new(),
            last_refresh: None,
            dirty: false,
            last_save: Instant::now(),
            form_url: cfg.as_ref().map(|c| c.base_url.clone()).unwrap_or_default(),
            form_token: String::new(),
            form_msg: None,
            syllabus_queue: Arc::default(),
            syllabus_worker: Arc::default(),
            syllabus_errors: HashMap::new(),
            syllabus_filter: String::new(),
            prefs: store::load_prefs(),
            exams: Vec::new(),
            exams_stale: true,
            revealed: HashSet::new(),
            tray: None,
            tray_rx: {
                let (ttx, trx) = channel();
                let ctx = cc.egui_ctx.clone();
                MenuEvent::set_event_handler(Some(move |e| {
                    let _ = ttx.send(e);
                    ctx.request_repaint();
                }));
                trx
            },
            tray_dirty: true,
            tray_built: Instant::now(),
            hidden: false,
            hidden_went_inactive: false,
        };
        if app.prefs.menu_bar {
            app.tray = Tray::new();
        }
        app.apply_appearance();
        app.refresh();
        app
    }

    // ---------- networking ----------

    fn spawn_with(&mut self, api: Arc<Api>, f: impl FnOnce(&Api) -> Msg + Send + 'static) {
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        self.inflight += 1;
        std::thread::spawn(move || {
            let _ = tx.send(f(&api));
            ctx.request_repaint();
        });
    }

    fn spawn(&mut self, f: impl FnOnce(&Api) -> Msg + Send + 'static) {
        if let Some(api) = self.api.clone() {
            self.spawn_with(api, f);
        }
    }

    fn refresh(&mut self) {
        if self.api.is_none() {
            return;
        }
        self.fresh.clear();
        self.error = None;
        self.last_refresh = Some(Instant::now());
        self.spawn(|a| Msg::Me(a.me().map_err(err)));
        self.spawn(|a| Msg::Courses(a.courses().map_err(err)));
        self.spawn(|a| Msg::Colors(a.colors().map_err(err)));
        self.spawn(|a| Msg::Missing(a.missing().map_err(err)));
        self.ensure_weeks();
    }

    /// Fetch the visible weeks plus their neighbours so arrowing around is instant.
    fn ensure_weeks(&mut self) {
        let mut mondays: Vec<NaiveDate> =
            [self.week_offset, self.week_offset + 1, self.week_offset - 1].into_iter().map(week_start).collect();
        if self.view == View::Calendar {
            let (first, last) = self.cal_range();
            let mut d = monday_of(first) - Duration::weeks(1);
            while d <= last + Duration::weeks(1) {
                mondays.push(d);
                d += Duration::weeks(1);
            }
        }
        for m in mondays {
            self.ensure_week(m);
        }
    }

    fn ensure_week(&mut self, monday: NaiveDate) {
        let key = week_key(monday);
        if self.fresh.insert(format!("w:{key}")) {
            let (s, e) = week_range(monday);
            self.spawn(move |a| Msg::Planner(key, a.planner(s, e).map_err(err)));
        }
    }

    /// First and last day shown by the calendar (whole weeks, Monday to Sunday).
    fn cal_range(&self) -> (NaiveDate, NaiveDate) {
        match self.cal_mode {
            CalMode::Week => {
                let m = monday_of(self.cal_anchor);
                (m, m + Duration::days(6))
            }
            CalMode::Month => {
                let first = self.cal_anchor.with_day(1).unwrap();
                let next = if first.month() == 12 {
                    NaiveDate::from_ymd_opt(first.year() + 1, 1, 1)
                } else {
                    NaiveDate::from_ymd_opt(first.year(), first.month() + 1, 1)
                }
                .unwrap();
                let last = next - Duration::days(1);
                (monday_of(first), monday_of(last) + Duration::days(6))
            }
        }
    }

    fn ensure_detail(&mut self, id: u64) {
        if self.fresh.insert(format!("c:{id}")) {
            self.spawn(move |a| Msg::Detail(id, a.course_detail(id).map_err(err)));
        }
    }

    fn handle(&mut self, msg: Msg) {
        // Syllabus scrapes run on their own worker and aren't counted in `inflight`.
        if !matches!(msg, Msg::Syllabus(..)) {
            self.inflight = self.inflight.saturating_sub(1);
        }
        self.dirty = true;
        self.exams_stale = true;
        self.tray_dirty = true;
        match msg {
            Msg::Connected(Ok((cfg, user))) => {
                if let Err(e) = store::save_config(&cfg) {
                    self.form_msg = Some((false, format!("Connected, but couldn't save config: {e}")));
                } else {
                    self.form_msg = Some((true, format!("Connected as {}", user.name)));
                }
                self.data = store::load_cache(&cfg.base_url);
                self.data.user_name = Some(user.name);
                self.base_url = cfg.base_url.clone();
                self.api = Api::new(&cfg).ok().map(Arc::new);
                self.form_token.clear();
                self.view = View::Upcoming;
                self.refresh();
            }
            Msg::Connected(Err(e)) => self.form_msg = Some((false, e)),
            Msg::Me(Ok(u)) => self.data.user_name = Some(u.name),
            Msg::Courses(Ok(c)) => {
                self.data.courses = c;
                self.data.updated_at = Some(Utc::now());
                // Prefetch every course page in the background.
                let ids: Vec<u64> = self.data.courses.iter().map(|c| c.id).collect();
                for id in ids {
                    self.ensure_detail(id);
                }
            }
            Msg::Colors(Ok(c)) => self.data.colors = c,
            Msg::Planner(k, Ok(items)) => {
                self.data.planner.insert(k, items);
            }
            Msg::Missing(Ok(m)) => self.data.missing = m,
            Msg::Detail(id, Ok(d)) => {
                self.data.details.insert(id, d);
                let mut ids = self.resolve_syllabus_tools();
                ids.push(id);
                for cid in ids {
                    if let Some(d) = self.data.details.get(&cid).cloned() {
                        self.queue_syllabus(cid, &d, false);
                    }
                }
            }
            Msg::Override { week, plannable_id, res } => {
                self.toggling.remove(&plannable_id);
                let item = self
                    .data
                    .planner
                    .get_mut(&week)
                    .and_then(|v| v.iter_mut().find(|i| i.plannable_id == plannable_id));
                match (res, item) {
                    (Ok(o), Some(item)) => item.planner_override = Some(o),
                    (Err(e), Some(item)) => {
                        // Roll back the optimistic toggle.
                        if let Some(o) = item.planner_override.as_mut() {
                            o.marked_complete = !o.marked_complete;
                            if o.id == 0 {
                                item.planner_override = None;
                            }
                        }
                        self.error = Some(e);
                    }
                    _ => {}
                }
            }
            Msg::Syllabus(id, res) => match res {
                Ok(s) => {
                    self.syllabus_errors.remove(&id);
                    self.data.syllabi.insert(id, s);
                }
                Err(e) => {
                    self.syllabus_errors.insert(id, e);
                }
            },
            Msg::Me(Err(e))
            | Msg::Courses(Err(e))
            | Msg::Planner(_, Err(e))
            | Msg::Missing(Err(e))
            | Msg::Detail(_, Err(e)) => {
                eprintln!("canvas-lite: {e}");
                self.error = Some(e);
            }
            // Colors are cosmetic; ignore failures.
            Msg::Colors(Err(_)) => {}
        }
    }

    /// Courses often share one syllabus tool (e.g. Simple Syllabus is tool 2042 school-wide).
    /// A course that only links that tool from its home page — or links it under a vague
    /// label — gets recognised by tool id. Returns the courses whose tool changed.
    fn resolve_syllabus_tools(&mut self) -> Vec<u64> {
        let known: HashMap<String, String> = self
            .data
            .details
            .values()
            .filter_map(|d| d.syllabus_tool.as_ref())
            .filter(|(label, _)| label != "Syllabus")
            .filter_map(|(label, href)| Some((tool_id(href)?.to_string(), label.clone())))
            .collect();
        let mut changed = Vec::new();
        for (cid, d) in self.data.details.iter_mut() {
            let found = match &d.syllabus_tool {
                Some((label, href)) if label == "Syllabus" => {
                    tool_id(href).and_then(|t| known.get(t)).map(|l| (l.clone(), href.clone()))
                }
                Some(_) => None,
                None => d
                    .home_tool_links
                    .iter()
                    .find_map(|(_, href)| known.get(tool_id(href)?).map(|l| (l.clone(), href.clone()))),
            };
            if let Some(t) = found {
                d.syllabus_tool = Some(t);
                changed.push(*cid);
            }
        }
        changed
    }

    /// Queue a scrape of the course's syllabus tool if Canvas's own syllabus is
    /// empty and our copy is missing or stale. `urgent` jumps the queue.
    fn queue_syllabus(&mut self, id: u64, d: &CourseDetail, urgent: bool) {
        let Some((label, path)) = d.syllabus_tool.clone() else { return };
        if d.syllabus.as_ref().is_some_and(|s| s.len() >= 400) {
            return;
        }
        let fresh = self
            .data
            .syllabi
            .get(&id)
            .is_some_and(|s| Utc::now() - s.fetched_at < Duration::days(SYLLABUS_MAX_AGE_DAYS));
        if fresh || self.syllabus_errors.contains_key(&id) && !urgent {
            return;
        }
        {
            let mut q = self.syllabus_queue.lock().unwrap();
            let queued = q.iter().position(|(c, ..)| *c == id);
            match (queued, urgent) {
                (Some(i), true) => {
                    let job = q.remove(i).unwrap();
                    q.push_front(job);
                }
                (Some(_), false) => {}
                (None, true) => q.push_front((id, label, path)),
                (None, false) => q.push_back((id, label, path)),
            }
        }
        self.syllabus_errors.remove(&id);
        self.start_syllabus_worker();
    }

    /// One worker scrapes queued syllabi one at a time (each is a hidden WebKit process).
    fn start_syllabus_worker(&mut self) {
        let Some(api) = self.api.clone() else { return };
        if self.syllabus_worker.swap(true, Ordering::SeqCst) {
            return;
        }
        let (queue, running, tx, ctx) =
            (self.syllabus_queue.clone(), self.syllabus_worker.clone(), self.tx.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            loop {
                let job = queue.lock().unwrap().pop_front();
                let Some((id, label, path)) = job else { break };
                let res = scrape_syllabus(&api, &label, &path);
                let _ = tx.send(Msg::Syllabus(id, res));
                ctx.request_repaint();
            }
            running.store(false, Ordering::SeqCst);
        });
    }

    fn syllabus_pending(&self, id: u64) -> bool {
        self.syllabus_queue.lock().unwrap().iter().any(|(c, ..)| *c == id)
    }

    fn apply_appearance(&self) {
        self.ctx.set_theme(match self.prefs.appearance.as_str() {
            "light" => egui::ThemePreference::Light,
            "dark" => egui::ThemePreference::Dark,
            _ => egui::ThemePreference::System,
        });
    }

    /// Merge exams from Canvas assignments and syllabus text; same course + day = one exam.
    fn compute_exams(&mut self) {
        let mut out: Vec<Exam> = Vec::new();
        for (&cid, d) in &self.data.details {
            for a in d.assignments.iter().filter(|a| crate::exams::is_exam_title(&a.name)) {
                let Some(due) = a.due_at.map(|t| t.with_timezone(&Local)) else { continue };
                out.push(Exam {
                    course_id: cid,
                    label: crate::exams::clean_label(&a.name),
                    date: due.date_naive(),
                    time: Some(due.time()),
                    canvas_id: Some(a.id),
                    url: a.html_url.clone(),
                    from_syllabus: false,
                    approx: false,
                });
            }
        }
        for (&cid, d) in &self.data.details {
            let text = self.data.syllabi.get(&cid).map(|s| s.text.as_str()).or(d.syllabus.as_deref());
            let Some(text) = text else { continue };
            let year = self
                .course(cid)
                .and_then(|c| c.term())
                .and_then(|t| t.rsplit(' ').next()?.parse().ok())
                .unwrap_or_else(|| Local::now().year());
            for f in crate::exams::from_syllabus(text, year) {
                match out.iter_mut().find(|e| e.course_id == cid && e.date == f.date) {
                    // The syllabus knows the start time and a clean name; Canvas knows the link.
                    Some(e) => {
                        e.from_syllabus = true;
                        e.label = f.label;
                        if f.time.is_some() {
                            e.time = f.time;
                        }
                    }
                    None => out.push(Exam {
                        course_id: cid,
                        label: f.label,
                        date: f.date,
                        time: f.time,
                        canvas_id: None,
                        url: None,
                        from_syllabus: true,
                        approx: f.approx,
                    }),
                }
            }
        }
        out.sort_by_key(|e| (e.date, e.time));
        self.exams = out;
        self.exams_stale = false;
    }

    /// The exam a planner item represents, if its title says it's one.
    fn item_exam(&self, it: &PlannerItem) -> Option<&Exam> {
        if !crate::exams::is_exam_title(it.title()) {
            return None;
        }
        self.exam_on(it.course_id, it.plannable_date.with_timezone(&Local).date_naive())
    }

    fn exam_on(&self, course_id: Option<u64>, date: NaiveDate) -> Option<&Exam> {
        let cid = course_id?;
        self.exams.iter().find(|e| e.course_id == cid && e.date == date)
    }

    /// Exams on `day` that have no Canvas item (those already appear as planner items).
    fn syllabus_exams_on(&self, day: NaiveDate) -> Vec<&Exam> {
        self.exams.iter().filter(|e| e.date == day && e.canvas_id.is_none()).collect()
    }

    fn grades_hidden(&self, course_id: u64) -> bool {
        self.prefs.hide_grades && !self.revealed.contains(&course_id)
    }

    fn set_menu_bar(&mut self, on: bool, show: &str) {
        self.prefs.menu_bar = on;
        self.prefs.menu_bar_show = show.to_string();
        store::save_prefs(&self.prefs);
        if on && self.tray.is_none() {
            self.tray = Tray::new();
        } else if !on {
            self.tray = None;
        }
        self.tray_dirty = true;
    }

    /// The menu bar model: up to two weeks of open items grouped by day, upcoming
    /// exams, then app actions. The title beside the glyph follows `menu_bar_show`.
    fn tray_model(&self) -> (Option<String>, String, Vec<Entry>) {
        let now = Local::now();
        let horizon = now + Duration::days(14);
        // (when, title, course, action)
        let mut next: Vec<(DateTime<Local>, String, String, TrayAction)> = Vec::new();
        let mut seen = HashSet::new();
        for items in self.data.planner.values() {
            for it in items {
                let t = it.plannable_date.with_timezone(&Local);
                if t < now || t > horizon || it.done() || !seen.insert((it.plannable_type.clone(), it.plannable_id)) {
                    continue;
                }
                let title = self
                    .item_exam(it)
                    .map(|e| e.label.clone())
                    .unwrap_or_else(|| crate::names::clean_title(it.title()));
                let action = it.html_url.clone().map(TrayAction::Open).unwrap_or(TrayAction::ShowWindow);
                next.push((t, title, self.course_label(it.course_id, it.context_name.as_deref()), action));
            }
        }
        for e in self.exams.iter().filter(|e| e.canvas_id.is_none()) {
            let t = e.date.and_time(e.time.unwrap_or(NaiveTime::from_hms_opt(23, 59, 0).unwrap()));
            let Some(t) = Local.from_local_datetime(&t).earliest() else { continue };
            if t >= now && t <= horizon {
                next.push((
                    t,
                    e.label.clone(),
                    self.course_label(Some(e.course_id), None),
                    TrayAction::Syllabus(e.course_id),
                ));
            }
        }
        next.sort_by_key(|n| n.0);
        next.truncate(14);

        let today = now.date_naive();
        let day_name = |d: NaiveDate| match (d - today).num_days() {
            0 => "Today".to_string(),
            1 => "Tomorrow".to_string(),
            _ => d.format("%A, %b %-d").to_string(),
        };
        let mut entries = Vec::new();
        let overdue =
            self.data.missing.iter().filter(|a| a.due_at.is_some_and(|d| d < now.with_timezone(&Utc))).count();
        if overdue > 0 {
            entries.push(Entry::Item { text: format!("{overdue} overdue"), action: TrayAction::ShowWindow });
            entries.push(Entry::Separator);
        }
        if next.is_empty() {
            entries.push(Entry::Header("Nothing due in the next two weeks".into()));
        }
        let mut last_day = None;
        for (t, title, course, action) in &next {
            let d = t.date_naive();
            if last_day != Some(d) {
                if last_day.is_some() {
                    entries.push(Entry::Separator);
                }
                entries.push(Entry::Header(day_name(d)));
                last_day = Some(d);
            }
            let mut name = title.clone();
            if name.chars().count() > 46 {
                name = name.chars().take(45).collect::<String>() + "…";
            }
            let course = if course.is_empty() { String::new() } else { format!("  ·  {course}") };
            entries.push(Entry::Item {
                text: format!("{}    {name}{course}", t.format("%-I:%M %p")),
                action: action.clone(),
            });
        }
        entries.push(Entry::Separator);
        entries.push(Entry::Item { text: "Open Canvas Lite".into(), action: TrayAction::ShowWindow });
        entries.push(Entry::Item { text: "Refresh".into(), action: TrayAction::Refresh });
        entries.push(Entry::Separator);
        entries.push(Entry::Item { text: "Quit Canvas Lite".into(), action: TrayAction::Quit });

        // Open items still due today / by Sunday (including ones earlier today not yet done).
        let week_end = monday_of(today) + Duration::days(6);
        let mut open_today = 0;
        let mut open_week = 0;
        let mut counted = HashSet::new();
        for it in self.data.planner.values().flatten() {
            let d = it.plannable_date.with_timezone(&Local).date_naive();
            if it.done() || d < today || d > week_end || !counted.insert((it.plannable_type.clone(), it.plannable_id)) {
                continue;
            }
            open_week += 1;
            open_today += (d == today) as usize;
        }
        for e in self.exams.iter().filter(|e| e.canvas_id.is_none() && e.date >= today && e.date <= week_end) {
            open_week += 1;
            open_today += (e.date == today) as usize;
        }
        let tooltip = format!("{open_today} due today · {open_week} left this week");
        let title = match self.prefs.menu_bar_show.as_str() {
            "today" => (open_today > 0).then(|| open_today.to_string()),
            "week" => (open_week > 0).then(|| open_week.to_string()),
            "next" => next.first().map(|(t, name, ..)| {
                let short: String = if name.chars().count() > 18 {
                    name.chars().take(17).collect::<String>() + "…"
                } else {
                    name.clone()
                };
                let when = if t.date_naive() == today { compact_time(t.time()) } else { t.format("%a").to_string() };
                format!("{short} · {when}")
            }),
            _ => None,
        };
        (title, tooltip, entries)
    }

    fn tray_action(&mut self, a: TrayAction) {
        match a {
            TrayAction::Open(path) => self.apply(vec![Action::Open(path)]),
            TrayAction::Syllabus(id) => {
                self.show_window();
                self.apply(vec![Action::CourseTab(id, Tab::Syllabus)]);
            }
            TrayAction::ShowWindow => {
                self.show_window();
                self.apply(vec![Action::View(View::Upcoming)]);
            }
            TrayAction::Refresh => self.refresh(),
            TrayAction::Quit => {
                self.persist();
                std::process::exit(0);
            }
        }
    }

    fn show_window(&mut self) {
        if self.hidden {
            self.hidden = false;
            crate::tray::set_dock_visible(true);
            self.ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        }
        self.ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }

    /// Write cache and the freshest session cookies to disk.
    fn persist(&self) {
        let Some(api) = &self.api else { return };
        store::save_cache(&self.base_url, &self.data);
        // Canvas refreshes session cookies as we go; keep the newest ones for next launch.
        if api.uses_session()
            && let Some(cookies) = api.cookie_header()
        {
            let _ = store::save_config(&Config { base_url: self.base_url.clone(), token: String::new(), cookies });
        }
    }

    fn set_appearance(&mut self, a: &str) {
        self.prefs.appearance = a.to_string();
        store::save_prefs(&self.prefs);
        self.apply_appearance();
    }

    fn apply(&mut self, actions: Vec<Action>) {
        for a in actions {
            match a {
                Action::Open(path) => {
                    if let Some(api) = &self.api {
                        let _ = open::that_detached(api.web_url(&path));
                    }
                }
                Action::View(v) => {
                    if let View::Course(id) = v {
                        self.ensure_detail(id);
                        if self.view != v {
                            self.tab = Tab::Overview;
                        }
                    }
                    self.view = v;
                    self.ensure_weeks();
                }
                Action::Tab(t) => {
                    self.tab = t;
                    if let (Tab::Syllabus, View::Course(id)) = (t, self.view)
                        && let Some(d) = self.data.details.get(&id).cloned()
                    {
                        self.queue_syllabus(id, &d, true);
                    }
                }
                Action::ShiftWeek(d) => {
                    self.week_offset = if d == 0 { 0 } else { self.week_offset + d };
                    self.view = View::Upcoming;
                    self.ensure_weeks();
                }
                Action::CalShift(d) => {
                    self.cal_anchor = match (d, self.cal_mode) {
                        (0, _) => Local::now().date_naive(),
                        (_, CalMode::Week) => self.cal_anchor + Duration::weeks(d),
                        (_, CalMode::Month) => {
                            let first = self.cal_anchor.with_day(1).unwrap();
                            if d > 0 {
                                first + chrono::Months::new(d as u32)
                            } else {
                                first - chrono::Months::new((-d) as u32)
                            }
                        }
                    };
                    self.ensure_weeks();
                }
                Action::CalMode(m) => {
                    self.cal_mode = m;
                    self.ensure_weeks();
                }
                Action::CalGoto(date, m) => {
                    self.cal_anchor = date;
                    self.cal_mode = m;
                    self.view = View::Calendar;
                    self.ensure_weeks();
                }
                Action::CourseTab(id, t) => {
                    self.apply(vec![Action::View(View::Course(id)), Action::Tab(t)]);
                }
                Action::RevealGrade(id) => {
                    self.revealed.insert(id);
                }
                Action::Refresh => self.refresh(),
                Action::Toggle { week, plannable_id } => self.toggle(week, plannable_id),
            }
        }
    }

    fn toggle(&mut self, week: String, plannable_id: u64) {
        if !self.toggling.insert(plannable_id) {
            return;
        }
        let Some(item) =
            self.data.planner.get_mut(&week).and_then(|v| v.iter_mut().find(|i| i.plannable_id == plannable_id))
        else {
            return;
        };
        let snapshot = item.clone();
        let done = !item.marked_complete();
        // Optimistic: flip locally right away, reconcile when Canvas answers.
        match item.planner_override.as_mut() {
            Some(o) => o.marked_complete = done,
            None => item.planner_override = Some(PlannerOverride { id: 0, marked_complete: done }),
        }
        self.dirty = true;
        self.spawn(move |a| Msg::Override { week, plannable_id, res: a.set_complete(&snapshot, done).map_err(err) });
    }

    /// Opens the school's login page in a separate WebKit window (see `web.rs`)
    /// and signs in with the session cookies it hands back.
    fn browser_login(&mut self) {
        let base = match Api::normalize_base(&self.form_url) {
            Ok(u) if !self.form_url.trim().is_empty() => u.to_string().trim_end_matches('/').to_string(),
            _ => {
                self.form_msg = Some((false, "Enter your Canvas URL first.".into()));
                return;
            }
        };
        self.form_msg = Some((true, "Finish signing in in the window that just opened…".into()));
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        self.inflight += 1;
        std::thread::spawn(move || {
            let res = (|| -> Result<(Config, User), String> {
                let exe = std::env::current_exe().map_err(|e| e.to_string())?;
                let out = std::process::Command::new(exe)
                    .args(["--login", &base])
                    .output()
                    .map_err(|e| format!("Couldn't open the sign-in window: {e}"))?;
                let line = String::from_utf8_lossy(&out.stdout);
                let cookies = serde_json::from_str::<serde_json::Value>(line.trim())
                    .ok()
                    .and_then(|v| v["cookies"].as_str().map(str::to_string))
                    .ok_or("Sign-in window was closed before finishing.")?;
                let cfg = Config { base_url: base, token: String::new(), cookies };
                let api = Api::new(&cfg).map_err(err)?;
                let user = api.me().map_err(err)?;
                // Store the cookies as they stand after that first request.
                let cfg = Config { cookies: api.cookie_header().unwrap_or(cfg.cookies), ..cfg };
                Ok((cfg, user))
            })();
            let _ = tx.send(Msg::Connected(res));
            ctx.request_repaint();
        });
    }

    fn connect(&mut self) {
        let cfg = Config {
            base_url: self.form_url.trim().to_string(),
            token: self.form_token.trim().to_string(),
            cookies: String::new(),
        };
        if cfg.base_url.is_empty() || cfg.token.is_empty() {
            self.form_msg = Some((false, "Enter both your Canvas URL and an access token.".into()));
            return;
        }
        match Api::new(&cfg) {
            Ok(api) => {
                self.form_msg = Some((true, "Connecting…".into()));
                self.spawn_with(Arc::new(api), move |a| Msg::Connected(a.me().map(|u| (cfg, u)).map_err(err)));
            }
            Err(e) => self.form_msg = Some((false, err(e))),
        }
    }

    // ---------- helpers ----------

    fn color(&self, course_id: Option<u64>) -> Color32 {
        course_id
            .and_then(|id| self.data.colors.get(&id))
            .and_then(|h| parse_hex(h))
            .unwrap_or_else(|| fallback_color(course_id.unwrap_or(0)))
    }

    fn course(&self, id: u64) -> Option<&Course> {
        self.data.courses.iter().find(|c| c.id == id)
    }

    fn course_label(&self, id: Option<u64>, fallback: Option<&str>) -> String {
        id.and_then(|id| self.course(id))
            .map(|c| c.short())
            .or(fallback.map(|n| crate::names::label(n, None)))
            .unwrap_or_default()
    }

    fn week_items(&self, offset: i64) -> Option<&Vec<PlannerItem>> {
        self.data.planner.get(&week_key(week_start(offset)))
    }

    /// Planner items due on `day`, in time order, with their week key (for toggling).
    fn day_items(&self, day: NaiveDate) -> (String, Vec<&PlannerItem>) {
        let key = week_key(monday_of(day));
        let mut v: Vec<&PlannerItem> = self
            .data
            .planner
            .get(&key)
            .map(|items| items.iter().filter(|i| i.plannable_date.with_timezone(&Local).date_naive() == day).collect())
            .unwrap_or_default();
        v.sort_by_key(|i| i.plannable_date);
        (key, v)
    }

    fn shortcuts(&self, ctx: &egui::Context, out: &mut Vec<Action>) {
        if ctx.egui_wants_keyboard_input() || self.view == View::Settings {
            return;
        }
        let cal = self.view == View::Calendar;
        ctx.input(|i| {
            let shift = |d: i64| if cal { Action::CalShift(d) } else { Action::ShiftWeek(d) };
            if i.key_pressed(Key::ArrowLeft) {
                out.push(shift(-1));
            }
            if i.key_pressed(Key::ArrowRight) {
                out.push(shift(1));
            }
            if i.key_pressed(Key::T) {
                out.push(shift(0));
            }
            if i.key_pressed(Key::Escape) || i.key_pressed(Key::U) {
                out.push(Action::View(View::Upcoming));
            }
            if i.key_pressed(Key::C) {
                out.push(Action::View(View::Calendar));
            }
            if cal && i.key_pressed(Key::W) {
                out.push(Action::CalMode(CalMode::Week));
            }
            if cal && i.key_pressed(Key::M) {
                out.push(Action::CalMode(CalMode::Month));
            }
            if i.key_pressed(Key::R) {
                out.push(Action::Refresh);
            }
            let nums =
                [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6, Key::Num7, Key::Num8, Key::Num9];
            for (n, k) in nums.iter().enumerate() {
                if i.key_pressed(*k)
                    && let Some(c) = self.data.courses.get(n)
                {
                    out.push(Action::View(View::Course(c.id)));
                }
            }
        });
    }
}

impl eframe::App for App {
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {
        while let Ok(m) = self.rx.try_recv() {
            self.handle(m);
        }
        if self.exams_stale {
            self.compute_exams();
        }
        // Hidden in the menu bar and the user reopens the app (Spotlight, Finder,
        // Launchpad): bring the window back. Poll gently while hidden.
        if self.hidden {
            let active = crate::tray::app_is_active();
            if active && self.hidden_went_inactive {
                self.show_window();
            } else {
                self.hidden_went_inactive |= !active;
                self.ctx.request_repaint_after(std::time::Duration::from_millis(400));
            }
        }
        while let Ok(ev) = self.tray_rx.try_recv() {
            if let Some(a) = self.tray.as_ref().and_then(|t| t.action(&ev)) {
                self.tray_action(a);
            }
        }
        // Rebuild the menu when data changed, and every minute so "Today"/times stay right.
        if self.tray.is_some() && (self.tray_dirty || self.tray_built.elapsed().as_secs() >= 60) && !self.exams_stale {
            let (title, tooltip, entries) = self.tray_model();
            if let Some(t) = self.tray.as_mut() {
                t.update(title.as_deref(), &tooltip, entries);
            }
            self.tray_dirty = false;
            self.tray_built = Instant::now();
        }
        if self.last_refresh.is_some_and(|t| t.elapsed().as_secs() > AUTO_REFRESH_SECS) {
            self.refresh();
        }
        // Debounced background cache write.
        if self.dirty && self.inflight == 0 && self.last_save.elapsed().as_secs() >= 2 {
            self.dirty = false;
            self.last_save = Instant::now();
            let (base, snapshot) = (self.base_url.clone(), self.data.clone());
            std::thread::spawn(move || store::save_cache(&base, &snapshot));
        }
        self.ctx.request_repaint_after(std::time::Duration::from_secs(30));
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // With the menu bar item on, closing the window just hides it (and the Dock icon).
        if ui.ctx().input(|i| i.viewport().close_requested()) && self.tray.is_some() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Visible(false));
            crate::tray::set_dock_visible(false);
            self.hidden = true;
            self.hidden_went_inactive = false;
            self.persist();
        }
        let mut out = Vec::new();
        self.shortcuts(&ui.ctx().clone(), &mut out);
        self.frame(ui, &mut out);
        self.apply(out);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.persist();
    }
}

// ---------- helpers ----------

fn scrape_syllabus(api: &Api, label: &str, path: &str) -> Res<Syllabus> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut cmd = std::process::Command::new(exe);
    cmd.args(["--extract", &api.web_url(path)]);
    if let Some(c) = api.cookie_header() {
        cmd.env("CANVAS_LITE_COOKIES", c);
    }
    let out = cmd.output().map_err(|e| format!("Couldn't load {label}: {e}"))?;
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).map_err(|_| format!("Couldn't load {label}."))?;
    if v["error"] == "login" {
        return Err(format!(
            "{label} needs a Canvas browser session. Sign in with browser in Settings, or open it in Canvas."
        ));
    }
    let text = crate::html::clean_page_text(v["text"].as_str().unwrap_or_default());
    if text.len() < 200 {
        return Err(format!("{label} didn't return a syllabus for this course."));
    }
    let url = v["url"].as_str().map(|u| u.split('?').next().unwrap_or(u).to_string());
    Ok(Syllabus { source: label.to_string(), url, text, fetched_at: Utc::now() })
}

/// "…/external_tools/2042?x" -> "2042"
fn tool_id(href: &str) -> Option<&str> {
    let rest = href.split("/external_tools/").nth(1)?;
    let end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
    (end > 0).then(|| &rest[..end])
}

fn submitted(a: &Assignment) -> bool {
    a.submission.as_ref().is_some_and(|s| {
        s.submitted_at.is_some() || matches!(s.workflow_state.as_deref(), Some("graded" | "submitted"))
    })
}

// ---------- time & misc ----------

fn week_start(offset: i64) -> NaiveDate {
    monday_of(Local::now().date_naive()) + Duration::weeks(offset)
}

fn monday_of(d: NaiveDate) -> NaiveDate {
    d - Duration::days(d.weekday().num_days_from_monday() as i64)
}

fn week_key(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

fn local_midnight(d: NaiveDate) -> DateTime<Utc> {
    Local
        .from_local_datetime(&d.and_hms_opt(0, 0, 0).unwrap())
        .earliest()
        .map(|t| t.with_timezone(&Utc))
        .unwrap_or_else(Utc::now)
}

fn week_range(start: NaiveDate) -> (DateTime<Utc>, DateTime<Utc>) {
    (local_midnight(start), local_midnight(start + Duration::days(7)))
}

/// "11:59p" — compact enough for calendar blocks.
fn compact_time(t: NaiveTime) -> String {
    use chrono::Timelike;
    format!("{}{}", t.format("%-I:%M"), if t.hour() < 12 { "a" } else { "p" })
}

fn ago(t: DateTime<Utc>) -> String {
    let s = (Utc::now() - t).num_seconds().max(0);
    match s {
        0..60 => "just now".into(),
        60..3600 => format!("{}m ago", s / 60),
        3600..86400 => format!("{}h ago", s / 3600),
        _ => format!("{}d ago", s / 86400),
    }
}

fn fmt_num(n: f64) -> String {
    if n.fract() == 0.0 { format!("{n:.0}") } else { format!("{n:.1}") }
}

fn err(e: anyhow::Error) -> String {
    format!("{e:#}")
}

fn parse_hex(h: &str) -> Option<Color32> {
    let h = h.trim_start_matches('#');
    if h.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(h, 16).ok()?;
    Some(Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

fn fallback_color(id: u64) -> Color32 {
    const P: [Color32; 8] = [
        Color32::from_rgb(99, 140, 255),
        Color32::from_rgb(236, 112, 99),
        Color32::from_rgb(88, 190, 140),
        Color32::from_rgb(240, 170, 70),
        Color32::from_rgb(170, 120, 230),
        Color32::from_rgb(70, 190, 210),
        Color32::from_rgb(230, 110, 170),
        Color32::from_rgb(150, 160, 90),
    ];
    P[(id % P.len() as u64) as usize]
}
