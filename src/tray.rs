//! Menu bar item: a template glyph (plus the next thing due, optionally) that opens a
//! native menu of what's coming up. Built from a plain model so `app` owns the content.

use std::collections::HashMap;
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

/// What a menu item does when chosen.
#[derive(Clone, Debug)]
pub enum TrayAction {
    /// Open a Canvas path/URL in the browser.
    Open(String),
    /// Show the window on a course's syllabus (exams without a Canvas item).
    Syllabus(u64),
    ShowWindow,
    Refresh,
    /// The session expired and silent renewal failed: sign in again.
    Reauth,
    Quit,
}

pub enum Entry {
    Header(String),
    Item { text: String, action: TrayAction },
    Separator,
}

pub struct Tray {
    icon: TrayIcon,
    actions: HashMap<MenuId, TrayAction>,
}

impl Tray {
    pub fn new() -> Option<Self> {
        let icon = TrayIconBuilder::new()
            .with_icon_templated(glyph())
            .with_tooltip("Canvas Lite")
            .with_menu_on_left_click(true)
            .build()
            .map_err(|e| eprintln!("canvas-lite: menu bar item unavailable: {e}"))
            .ok()?;
        Some(Self { icon, actions: HashMap::new() })
    }

    pub fn update(&mut self, title: Option<&str>, tooltip: &str, entries: Vec<Entry>) {
        let menu = Menu::new();
        self.actions.clear();
        for e in entries {
            let _ = match e {
                Entry::Header(t) => menu.append(&MenuItem::new(t, false, None)),
                Entry::Separator => menu.append(&PredefinedMenuItem::separator()),
                Entry::Item { text, action } => {
                    let item = MenuItem::new(text, true, None);
                    self.actions.insert(item.id().clone(), action);
                    menu.append(&item)
                }
            };
        }
        self.icon.set_menu(Some(Box::new(menu)));
        self.icon.set_title(title);
        let _ = self.icon.set_tooltip(Some(tooltip));
    }

    pub fn action(&self, ev: &MenuEvent) -> Option<TrayAction> {
        self.actions.get(ev.id()).cloned()
    }
}

/// Show or hide the Dock icon. Hidden while only the menu bar item is open.
pub fn set_dock_visible(visible: bool) {
    #[cfg(target_os = "macos")]
    {
        use objc2::MainThreadMarker;
        use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
        let Some(mtm) = MainThreadMarker::new() else { return };
        let app = NSApplication::sharedApplication(mtm);
        app.setActivationPolicy(if visible {
            NSApplicationActivationPolicy::Regular
        } else {
            NSApplicationActivationPolicy::Accessory
        });
        if visible {
            app.activate();
        }
    }
}

/// Whether the app is frontmost (e.g. just relaunched from Spotlight or Finder).
pub fn app_is_active() -> bool {
    #[cfg(target_os = "macos")]
    {
        use objc2::MainThreadMarker;
        use objc2_app_kit::NSApplication;
        if let Some(mtm) = MainThreadMarker::new() {
            return NSApplication::sharedApplication(mtm).isActive();
        }
    }
    false
}

/// Template glyph, 18×18 pt drawn at 2× (36 px). The page occupies only the middle
/// ~12 pt so it sits at the same optical size as system menu bar icons, with the
/// standard breathing room around it.
fn glyph() -> Icon {
    const S: usize = 36;
    let mut px = vec![0u8; S * S * 4];
    let mut set = |x: usize, y: usize| {
        if x < S && y < S {
            px[(y * S + x) * 4 + 3] = 255;
        }
    };
    // Page outline (2 px = 1 pt stroke): x 10..26, y 6..30, fold 6 px at top-right.
    let (l, t, r, b, f) = (10usize, 6usize, 25usize, 29usize, 6usize);
    for w in 0..2 {
        for x in l..=r - f {
            set(x, t + w);
        }
        for x in l..=r {
            set(x, b - w);
        }
        for y in t..=b {
            set(l + w, y);
        }
        for y in t + f..=b {
            set(r - w, y);
        }
        // Clipped corner: a 2 px diagonal (cleaner than a drawn fold at this size).
        for k in 0..=f {
            set(r - f + k, t + k + w);
        }
    }
    // Two list rows: dot + line, 1 pt tall.
    for (row, len) in [(15usize, 6usize), (21, 4)] {
        for dy in 0..2 {
            for x in 14..16 {
                set(x, row + dy);
            }
            for x in 18..18 + len {
                set(x, row + dy);
            }
        }
    }
    Icon::from_rgba(px, S as u32, S as u32).expect("tray glyph")
}
