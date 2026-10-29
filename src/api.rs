//! Minimal Canvas REST client. Only the endpoints the app actually uses.

use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Utc};
use reqwest::Url;
use reqwest::blocking::Client;
use reqwest::cookie::{CookieStore, Jar};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

/// Error text for a browser session Canvas no longer accepts; the app renews on seeing it.
pub const SESSION_EXPIRED: &str = "Canvas session expired";

pub struct Api {
    base: Url,
    /// Empty when signed in through the browser; then `jar` carries the session.
    token: String,
    jar: Arc<Jar>,
    http: Client,
}

// ---------- models ----------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: u64,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Enrollment {
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub computed_current_score: Option<f64>,
    #[serde(default)]
    pub computed_current_grade: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Course {
    pub id: u64,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub course_code: Option<String>,
    #[serde(default)]
    pub enrollments: Vec<Enrollment>,
}

impl Course {
    pub fn full_name(&self) -> &str {
        self.name.as_deref().unwrap_or("Untitled course")
    }
    /// "COMP 120"-style label for tight spaces.
    pub fn short(&self) -> String {
        crate::names::label(self.full_name(), self.course_code.as_deref())
    }
    /// Readable title, e.g. "Program Design & Concepts".
    pub fn title(&self) -> String {
        crate::names::title(self.full_name())
    }
    pub fn term(&self) -> Option<String> {
        crate::names::term(self.full_name())
    }
    pub fn score(&self) -> Option<f64> {
        self.enrollments.iter().find_map(|e| e.computed_current_score)
    }
    pub fn grade(&self) -> Option<&str> {
        self.enrollments.iter().find_map(|e| e.computed_current_grade.as_deref())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannerOverride {
    pub id: u64,
    #[serde(default)]
    pub marked_complete: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Plannable {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub points_possible: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannerItem {
    pub plannable_id: u64,
    pub plannable_type: String,
    pub plannable_date: DateTime<Utc>,
    #[serde(default)]
    pub course_id: Option<u64>,
    #[serde(default)]
    pub context_name: Option<String>,
    #[serde(default)]
    pub html_url: Option<String>,
    #[serde(default)]
    pub plannable: Plannable,
    /// Canvas sends `false` or an object here, so keep it loose.
    #[serde(default)]
    pub submissions: serde_json::Value,
    #[serde(default)]
    pub planner_override: Option<PlannerOverride>,
}

impl PlannerItem {
    fn sub_flag(&self, k: &str) -> bool {
        self.submissions.get(k).and_then(|v| v.as_bool()).unwrap_or(false)
    }
    pub fn submitted(&self) -> bool {
        self.sub_flag("submitted")
    }
    pub fn graded(&self) -> bool {
        self.sub_flag("graded")
    }
    pub fn missing(&self) -> bool {
        self.sub_flag("missing")
    }
    pub fn late(&self) -> bool {
        self.sub_flag("late")
    }
    pub fn marked_complete(&self) -> bool {
        self.planner_override.as_ref().is_some_and(|o| o.marked_complete)
    }
    pub fn done(&self) -> bool {
        self.marked_complete() || self.submitted() || self.graded()
    }
    pub fn title(&self) -> &str {
        self.plannable.title.as_deref().unwrap_or("Untitled")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Submission {
    #[serde(default)]
    pub workflow_state: Option<String>,
    #[serde(default)]
    pub score: Option<f64>,
    #[serde(default)]
    pub submitted_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub missing: bool,
    #[serde(default)]
    pub late: bool,
    #[serde(default)]
    pub excused: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Assignment {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub course_id: Option<u64>,
    #[serde(default)]
    pub due_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub points_possible: Option<f64>,
    #[serde(default)]
    pub html_url: Option<String>,
    #[serde(default)]
    pub submission: Option<Submission>,
    #[serde(default)]
    pub assignment_group_id: Option<u64>,
    #[serde(default)]
    pub omit_from_final_grade: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssignmentGroup {
    pub id: u64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub position: i64,
    #[serde(default)]
    pub group_weight: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Author {
    #[serde(default)]
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Announcement {
    pub id: u64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub posted_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub html_url: Option<String>,
    #[serde(default)]
    pub author: Option<Author>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleItem {
    #[serde(default)]
    pub title: String,
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub html_url: Option<String>,
    #[serde(default)]
    pub indent: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Module {
    pub id: u64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub items: Vec<ModuleItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CourseWithSyllabus {
    #[serde(default)]
    syllabus_body: Option<String>,
    #[serde(default)]
    apply_assignment_group_weights: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct Page {
    #[serde(default)]
    body: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct Tab {
    #[serde(default)]
    label: String,
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    html_url: Option<String>,
    #[serde(default)]
    hidden: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CourseDetail {
    /// Canvas's own syllabus body as plain text (often empty or just a link).
    pub syllabus: Option<String>,
    /// Links found in the syllabus body: (label, href).
    #[serde(default)]
    pub syllabus_links: Vec<(String, String)>,
    /// External syllabus tool tab (e.g. Simple Syllabus): (label, Canvas path).
    #[serde(default)]
    pub syllabus_tool: Option<(String, String)>,
    /// External-tool links on the course home page: (label, href). Some courses only
    /// link their syllabus tool from there (often as an image button).
    #[serde(default)]
    pub home_tool_links: Vec<(String, String)>,
    #[serde(default)]
    pub groups: Vec<AssignmentGroup>,
    #[serde(default)]
    pub weighted: bool,
    pub assignments: Vec<Assignment>,
    pub announcements: Vec<Announcement>,
    pub modules: Vec<Module>,
}

// ---------- client ----------

impl Api {
    pub fn normalize_base(base_url: &str) -> Result<Url> {
        let mut s = base_url.trim().trim_end_matches('/').to_string();
        if !s.starts_with("http") {
            s = format!("https://{s}");
        }
        Url::parse(&s).context("invalid Canvas URL")
    }

    pub fn new(cfg: &crate::store::Config) -> Result<Self> {
        let base = Self::normalize_base(&cfg.base_url)?;
        let jar = Arc::new(Jar::default());
        for c in cfg.cookies.split(';').map(str::trim).filter(|c| !c.is_empty()) {
            jar.add_cookie_str(&format!("{c}; Path=/"), &base);
        }
        let http = Client::builder()
            .timeout(Duration::from_secs(30))
            .pool_max_idle_per_host(16)
            .user_agent("canvas-lite/0.1")
            .cookie_provider(jar.clone())
            .build()?;
        Ok(Self { base, token: cfg.token.trim().to_string(), jar, http })
    }

    pub fn uses_session(&self) -> bool {
        self.token.is_empty()
    }

    /// Current session cookies (Canvas rotates them via Set-Cookie), to persist on exit.
    pub fn cookie_header(&self) -> Option<String> {
        self.jar.cookies(&self.base).and_then(|h| h.to_str().ok().map(str::to_string))
    }

    /// Canvas wants the (URL-decoded) `_csrf_token` cookie echoed on writes made with a session.
    fn csrf(&self) -> Option<String> {
        let all = self.cookie_header()?;
        let raw = all.split("; ").find_map(|c| c.strip_prefix("_csrf_token="))?;
        Some(url_decode(raw))
    }

    /// Turn a Canvas-relative path ("/courses/1/assignments/2") into a full URL.
    pub fn web_url(&self, path: &str) -> String {
        if path.starts_with("http") {
            path.to_string()
        } else {
            self.base.join(path).map(|u| u.to_string()).unwrap_or_default()
        }
    }

    fn url(&self, path: &str, query: &[(&str, &str)]) -> Result<Url> {
        let mut u = self.base.join(path)?;
        {
            let mut q = u.query_pairs_mut();
            for (k, v) in query {
                q.append_pair(k, v);
            }
        }
        Ok(u)
    }

    fn send<T: DeserializeOwned>(&self, req: reqwest::blocking::RequestBuilder) -> Result<(T, Option<String>)> {
        let req = if self.token.is_empty() {
            // Session auth. The XHR header stops Canvas from prefixing JSON with `while(1);`.
            let req = req.header("X-Requested-With", "XMLHttpRequest");
            match self.csrf() {
                Some(t) => req.header("X-CSRF-Token", t),
                None => req,
            }
        } else {
            req.bearer_auth(&self.token)
        };
        // One retry for flaky connections (sleeping laptops, reset pooled sockets).
        let resp = match req.try_clone().map(|r| r.send()) {
            Some(Err(e)) if e.is_connect() || e.is_timeout() || e.is_request() => req.send()?,
            Some(r) => r?,
            None => req.send()?,
        };
        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(anyhow!(if self.token.is_empty() {
                SESSION_EXPIRED
            } else {
                "Canvas rejected the access token (401)"
            }));
        }
        if !status.is_success() {
            return Err(anyhow!("Canvas returned HTTP {status}"));
        }
        let next = resp.headers().get("link").and_then(|h| h.to_str().ok()).and_then(next_link);
        let body = resp.text()?;
        let json = body.strip_prefix("while(1);").unwrap_or(&body);
        Ok((serde_json::from_str(json).context("unexpected response from Canvas")?, next))
    }

    fn get<T: DeserializeOwned>(&self, path: &str, query: &[(&str, &str)]) -> Result<T> {
        Ok(self.send(self.http.get(self.url(path, query)?))?.0)
    }

    /// Follows `Link: rel="next"` up to `max_pages`.
    fn get_all<T: DeserializeOwned>(&self, path: &str, query: &[(&str, &str)], max_pages: usize) -> Result<Vec<T>> {
        let mut out = Vec::new();
        let mut url = Some(self.url(path, query)?.to_string());
        let mut pages = 0;
        while let Some(u) = url.take() {
            let (mut page, next): (Vec<T>, _) = self.send(self.http.get(&u))?;
            out.append(&mut page);
            pages += 1;
            if pages < max_pages {
                url = next;
            }
        }
        Ok(out)
    }

    pub fn me(&self) -> Result<User> {
        self.get("/api/v1/users/self", &[])
    }

    /// Dashboard (favorited) courses, falling back to all active ones.
    pub fn courses(&self) -> Result<Vec<Course>> {
        let q = [("include[]", "total_scores"), ("per_page", "100")];
        let mut c: Vec<Course> = self.get_all("/api/v1/users/self/favorites/courses", &q, 3)?;
        if c.is_empty() {
            let q = [("enrollment_state", "active"), ("include[]", "total_scores"), ("per_page", "100")];
            c = self.get_all("/api/v1/courses", &q, 3)?;
        }
        c.retain(|c| c.name.is_some());
        Ok(c)
    }

    /// course_id -> "#rrggbb" from the user's Canvas color choices.
    pub fn colors(&self) -> Result<HashMap<u64, String>> {
        #[derive(Deserialize)]
        struct Colors {
            custom_colors: HashMap<String, String>,
        }
        let c: Colors = self.get("/api/v1/users/self/colors", &[])?;
        Ok(c.custom_colors
            .into_iter()
            .filter_map(|(k, v)| Some((k.strip_prefix("course_")?.parse().ok()?, v)))
            .collect())
    }

    pub fn planner(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Result<Vec<PlannerItem>> {
        let (s, e) = (start.to_rfc3339(), end.to_rfc3339());
        let q = [("start_date", s.as_str()), ("end_date", e.as_str()), ("per_page", "100")];
        let mut items: Vec<PlannerItem> = self.get_all("/api/v1/planner/items", &q, 5)?;
        items.retain(|i| i.plannable_type != "announcement");
        Ok(items)
    }

    pub fn missing(&self) -> Result<Vec<Assignment>> {
        let q = [("filter[]", "submittable"), ("per_page", "100")];
        self.get_all("/api/v1/users/self/missing_submissions", &q, 3)
    }

    pub fn course_detail(&self, course_id: u64) -> Result<CourseDetail> {
        let cid = course_id.to_string();
        // Fire the requests in parallel; they share one connection pool.
        std::thread::scope(|s| {
            let syl = s.spawn(|| {
                self.get::<CourseWithSyllabus>(&format!("/api/v1/courses/{cid}"), &[("include[]", "syllabus_body")])
            });
            let asg = s.spawn(|| {
                self.get_all::<Assignment>(
                    &format!("/api/v1/courses/{cid}/assignments"),
                    &[("include[]", "submission"), ("order_by", "due_at"), ("per_page", "100")],
                    5,
                )
            });
            // Not /api/v1/announcements: its end_date silently defaults to start_date + 28 days.
            let ann = s.spawn(|| {
                self.get_all::<Announcement>(
                    &format!("/api/v1/courses/{cid}/discussion_topics"),
                    &[("only_announcements", "true"), ("per_page", "30")],
                    1,
                )
            });
            let tabs = s.spawn(|| self.get::<Vec<Tab>>(&format!("/api/v1/courses/{cid}/tabs"), &[]));
            let home = s.spawn(|| self.get::<Page>(&format!("/api/v1/courses/{cid}/front_page"), &[]));
            let groups = s.spawn(|| {
                self.get_all::<AssignmentGroup>(
                    &format!("/api/v1/courses/{cid}/assignment_groups"),
                    &[("per_page", "50")],
                    2,
                )
            });
            let modules = s.spawn(|| {
                self.get_all::<Module>(
                    &format!("/api/v1/courses/{cid}/modules"),
                    &[("include[]", "items"), ("per_page", "50")],
                    4,
                )
            });
            let course = syl.join().unwrap().ok();
            let weighted = course.as_ref().is_some_and(|c| c.apply_assignment_group_weights);
            let body = course.and_then(|c| c.syllabus_body).unwrap_or_default();
            let home_tool_links: Vec<(String, String)> = home
                .join()
                .unwrap()
                .ok()
                .and_then(|p| p.body)
                .map(|b| crate::html::links(&b))
                .unwrap_or_default()
                .into_iter()
                .filter(|(_, h)| h.contains("/external_tools/"))
                .collect();
            let syllabus_links = crate::html::links(&body);
            let syllabus = Some(crate::html::to_text(&body)).filter(|t| !t.trim().is_empty());
            let tabs = tabs.join().unwrap().unwrap_or_default();
            // A visible external-tool tab that looks like a syllabus, else a tool linked from the body.
            let syllabus_tool = tabs
                .iter()
                .find(|t| t.kind == "external" && t.hidden != Some(true) && t.label.to_lowercase().contains("syllabus"))
                .and_then(|t| Some((t.label.clone(), t.html_url.clone()?)))
                .or_else(|| {
                    syllabus_links
                        .iter()
                        .find(|(_, href)| href.contains("/external_tools/"))
                        .map(|(label, href)| (label.clone(), href.clone()))
                })
                .or_else(|| {
                    home_tool_links
                        .iter()
                        .find(|(label, _)| label.to_lowercase().contains("syllab"))
                        .map(|(_, href)| ("Syllabus".to_string(), href.clone()))
                });
            let mut groups = groups.join().unwrap().unwrap_or_default();
            groups.sort_by_key(|g| g.position);
            Ok(CourseDetail {
                syllabus,
                syllabus_links,
                syllabus_tool,
                home_tool_links,
                groups,
                weighted,
                assignments: asg.join().unwrap()?,
                // Some courses hide announcements/modules; treat that as empty rather than fatal.
                // Bodies are converted to text once here so rendering never touches HTML.
                announcements: ann
                    .join()
                    .unwrap()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|mut a| {
                        a.message = a.message.map(|m| crate::html::to_text(&m));
                        a
                    })
                    .collect(),
                modules: modules.join().unwrap().unwrap_or_default(),
            })
        })
    }

    pub fn set_complete(&self, item: &PlannerItem, done: bool) -> Result<PlannerOverride> {
        let body = serde_json::json!({ "marked_complete": done });
        let req = match &item.planner_override {
            Some(o) => self.http.put(self.url(&format!("/api/v1/planner/overrides/{}", o.id), &[])?).json(&body),
            None => self.http.post(self.url("/api/v1/planner/overrides", &[])?).json(&serde_json::json!({
                "plannable_type": item.plannable_type,
                "plannable_id": item.plannable_id,
                "marked_complete": done,
            })),
        };
        Ok(self.send(req)?.0)
    }
}

fn url_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' if i + 2 < b.len() => match u8::from_str_radix(&s[i + 1..i + 3], 16) {
                Ok(v) => {
                    out.push(v);
                    i += 3;
                    continue;
                }
                Err(_) => out.push(b'%'),
            },
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn next_link(header: &str) -> Option<String> {
    header.split(',').find_map(|part| {
        let (url, rel) = part.split_once(';')?;
        rel.contains("rel=\"next\"").then(|| url.trim().trim_start_matches('<').trim_end_matches('>').to_string())
    })
}
