//! Helper WebKit windows, each run as its own process:
//!
//! `canvas-lite --login <base-url>`: a sign-in window for schools that
//! don't allow personal access tokens. The user signs in normally (SSO, Duo, …);
//! once the browser lands back on a Canvas page outside `/login`, the session
//! cookies are printed to stdout as JSON and the process exits.
//!
//! `canvas-lite --extract <url>`: loads a Canvas external-tool page (e.g. Simple
//! Syllabus) in a hidden window and prints the tool's rendered text.
//!
//! They run as separate processes because macOS allows only one native event
//! loop per process, and eframe already owns the main one.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop};
use tao::window::WindowBuilder;
use wry::WebViewBuilder;

pub fn login(base: &str) {
    let base = base.trim_end_matches('/').to_string();
    let host = reqwest::Url::parse(&base).ok().and_then(|u| u.host_str().map(str::to_string)).unwrap_or_default();

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("Sign in to Canvas")
        .with_inner_size(tao::dpi::LogicalSize::new(920.0, 780.0))
        .build(&event_loop)
        .expect("login window");
    let webview = WebViewBuilder::new().with_url(format!("{base}/login")).build(&window).expect("webview");
    window.set_focus();

    let mut next_check = Instant::now();
    event_loop.run(move |event, _, flow| {
        *flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(400));
        if let Event::WindowEvent { event: WindowEvent::CloseRequested, .. } = event {
            std::process::exit(1);
        }
        if Instant::now() < next_check {
            return;
        }
        next_check = Instant::now() + Duration::from_millis(400);

        let Some(url) = webview.url().ok().and_then(|u| reqwest::Url::parse(&u).ok()) else { return };
        let signed_in = url.host_str() == Some(host.as_str()) && !url.path().starts_with("/login");
        if !signed_in {
            return;
        }
        let Ok(cookies) = webview.cookies_for_url(&base) else { return };
        if !cookies.iter().any(|c| c.name().contains("session")) {
            return;
        }
        let header = cookies.iter().map(|c| format!("{}={}", c.name(), c.value())).collect::<Vec<_>>().join("; ");
        println!("{}", serde_json::json!({ "cookies": header }));
        std::process::exit(0);
    });
}

/// Canvas shows LTI tools in an iframe fed by `form#tool_form`. Retarget that form
/// to the top window so the tool's page *is* the document and its text is readable.
const EXTRACT_JS: &str = r#"(() => {
  if (location.host === __HOST__) {
    if (location.pathname.startsWith('/login')) return JSON.stringify({ state: 'login' });
    const f = document.querySelector('form#tool_form') || document.querySelector('form[target^="tool_content"]');
    if (f && !window.__canvasLite) { window.__canvasLite = 1; f.target = '_self'; f.submit(); }
    return JSON.stringify({ state: 'launching' });
  }
  const t = document.body ? document.body.innerText : '';
  return JSON.stringify({ state: 'tool', url: location.href, text: t });
})()"#;

pub fn extract(url: &str) {
    let parsed = reqwest::Url::parse(url).expect("url");
    let host = parsed.host_str().unwrap_or_default().to_string();
    let js = EXTRACT_JS.replace("__HOST__", &serde_json::to_string(&host).unwrap());

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_visible(false)
        .with_inner_size(tao::dpi::LogicalSize::new(1000.0, 1400.0))
        .build(&event_loop)
        .expect("window");
    let webview = WebViewBuilder::new().build(&window).expect("webview");
    // Seed the session so this works even if the webview's own cookie store is empty.
    if let Ok(cookies) = std::env::var("CANVAS_LITE_COOKIES") {
        for c in cookies.split("; ").filter_map(|c| c.split_once('=')) {
            let ck =
                cookie::Cookie::build((c.0.to_string(), c.1.to_string())).domain(host.clone()).path("/").secure(true);
            let _ = webview.set_cookie(&ck.build());
        }
    }
    let _ = webview.load_url(url);

    let latest: Arc<Mutex<Option<serde_json::Value>>> = Arc::default();
    let started = Instant::now();
    let mut next_poll = Instant::now() + Duration::from_millis(1500);
    let (mut last_text, mut stable) = (String::new(), 0);
    event_loop.run(move |_event, _, flow| {
        *flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(250));
        if Instant::now() < next_poll {
            return;
        }
        next_poll = Instant::now() + Duration::from_millis(1000);

        let slot = latest.clone();
        let _ = webview.evaluate_script_with_callback(&js, move |res| {
            // The callback gets the JS return value JSON-encoded, i.e. a JSON string of JSON.
            let v = serde_json::from_str::<String>(&res).ok().and_then(|s| serde_json::from_str(&s).ok());
            *slot.lock().unwrap() = v;
        });

        let snapshot = latest.lock().unwrap().clone();
        if let Some(v) = &snapshot {
            match v["state"].as_str() {
                Some("login") => finish(serde_json::json!({ "error": "login" })),
                Some("tool") => {
                    let text = v["text"].as_str().unwrap_or_default().to_string();
                    // Done once the rendered text stops changing for a couple of polls.
                    if text.len() > 200 && text == last_text {
                        stable += 1;
                        if stable >= 2 {
                            finish(serde_json::json!({ "url": v["url"], "text": text }));
                        }
                    } else {
                        stable = 0;
                    }
                    last_text = text;
                }
                _ => {}
            }
        }
        if started.elapsed() > Duration::from_secs(40) {
            let url = snapshot.as_ref().map(|v| v["url"].clone()).unwrap_or_default();
            finish(serde_json::json!({ "url": url, "text": last_text, "error": "timeout" }));
        }
    });
}

fn finish(v: serde_json::Value) -> ! {
    println!("{v}");
    std::process::exit(0);
}
