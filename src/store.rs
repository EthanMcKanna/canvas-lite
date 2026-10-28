//! Config (Canvas URL + token) and the on-disk cache that makes startup instant.

use crate::api::{Assignment, Course, CourseDetail, PlannerItem};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

const APP: &str = "canvas-lite";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    pub base_url: String,
    #[serde(default)]
    pub token: String,
    /// Session cookies from "Sign in with browser" (used when `token` is empty).
    #[serde(default)]
    pub cookies: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Cache {
    pub user_name: Option<String>,
    pub courses: Vec<Course>,
    pub colors: HashMap<u64, String>,
    /// Keyed by the week's Monday, "YYYY-MM-DD".
    pub planner: HashMap<String, Vec<PlannerItem>>,
    pub missing: Vec<Assignment>,
    pub details: HashMap<u64, CourseDetail>,
    /// Syllabi scraped from external tools like Simple Syllabus (slow to fetch, so kept apart).
    #[serde(default)]
    pub syllabi: HashMap<u64, Syllabus>,
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Syllabus {
    /// Where it came from, e.g. "Simple Syllabus".
    pub source: String,
    /// The tool's own page, for opening in a browser.
    pub url: Option<String>,
    pub text: String,
    pub fetched_at: chrono::DateTime<chrono::Utc>,
}

/// UI preferences, independent of which Canvas you're signed into.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prefs {
    /// "system", "light" or "dark".
    #[serde(default)]
    pub appearance: String,
    /// Hide overall course grades (sidebar, course header, Grades totals).
    #[serde(default)]
    pub hide_grades: bool,
    /// Hide individual assignment scores too.
    #[serde(default)]
    pub hide_scores: bool,
    /// Show a menu bar item with what's next.
    #[serde(default = "yes")]
    pub menu_bar: bool,
    /// What sits next to the menu bar glyph: "none", "today" (count due today),
    /// "week" (count left this week) or "next" (the next item's name).
    #[serde(default = "today")]
    pub menu_bar_show: String,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            appearance: String::new(),
            hide_grades: false,
            hide_scores: false,
            menu_bar: true,
            menu_bar_show: today(),
        }
    }
}

fn yes() -> bool {
    true
}

fn today() -> String {
    "today".into()
}

fn prefs_path() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join(APP).join("prefs.json"))
}

pub fn load_prefs() -> Prefs {
    prefs_path().and_then(|p| fs::read(p).ok()).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

pub fn save_prefs(p: &Prefs) {
    if let (Some(path), Ok(bytes)) = (prefs_path(), serde_json::to_vec_pretty(p)) {
        let _ = write_private(&path, &bytes);
    }
}

fn config_path() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join(APP).join("config.json"))
}

/// One cache per Canvas host, so switching schools (or the dev mock) never mixes data.
fn cache_path(base_url: &str) -> Option<PathBuf> {
    let host: String = base_url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '.' { c } else { '_' })
        .collect();
    Some(dirs::cache_dir()?.join(APP).join(format!("{host}.json")))
}

pub fn load_config() -> Option<Config> {
    // Env vars win, so the token never has to touch disk if you'd rather not.
    if let (Ok(base_url), Ok(token)) = (std::env::var("CANVAS_URL"), std::env::var("CANVAS_TOKEN")) {
        return Some(Config { base_url, token, cookies: String::new() });
    }
    let c: Config = serde_json::from_slice(&fs::read(config_path()?).ok()?).ok()?;
    (!c.token.is_empty() || !c.cookies.is_empty()).then_some(c)
}

pub fn save_config(c: &Config) -> std::io::Result<()> {
    let p = config_path().ok_or_else(|| std::io::Error::other("no config dir"))?;
    write_private(&p, &serde_json::to_vec_pretty(c)?)
}

pub fn clear_config(base_url: &str) {
    if let Some(p) = config_path() {
        let _ = fs::remove_file(p);
    }
    if let Some(p) = cache_path(base_url) {
        let _ = fs::remove_file(p);
    }
}

pub fn load_cache(base_url: &str) -> Cache {
    cache_path(base_url)
        .and_then(|p| fs::read(p).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn save_cache(base_url: &str, c: &Cache) {
    if let (Some(p), Ok(bytes)) = (cache_path(base_url), serde_json::to_vec(c)) {
        let _ = write_private(&p, &bytes);
    }
}

/// Atomic write with 0600 permissions (the config holds an API token).
fn write_private(path: &PathBuf, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600))?;
    }
    fs::rename(tmp, path)
}
