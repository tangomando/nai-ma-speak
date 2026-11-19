use crate::worker::{UiEvent, UiSender};
use std::thread;
use std::time::Duration;

/// A rectangle on screen as `[x, y, width, height]`, in the points the overlay is placed in.
pub type Rect = [f32; 4];

/// A window on screen.
// Listed on macOS only; elsewhere just the focused window is known.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Debug, Clone, PartialEq)]
pub struct Window {
    pub app: String,
    pub title: String,
    /// The system's window layer: 0 for ordinary windows, higher for the menu bar, the Dock
    /// and always-on-top windows such as the overlay itself.
    pub layer: i32,
    pub rect: Rect,
}

/// What can be seen of the WoW window, which decides whether the overlay shows.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum WowView {
    /// The window list cannot be read; the overlay then stays visible.
    #[default]
    Unknown,
    /// WoW is not on screen: closed, minimized, hidden or on another desktop.
    Hidden,
    /// WoW is on screen, with these ordinary windows in front of it.
    Shown { covers: Vec<Rect> },
}

impl WowView {
    /// Whether an overlay at `overlay` should show: WoW is on screen and no window in front
    /// of it reaches into the overlay's area. Like the game's own interface, the overlay
    /// goes away under a window that covers it, but stays while another program has focus
    /// elsewhere on the screen or on another screen.
    pub fn shows_overlay(&self, overlay: Option<Rect>) -> bool {
        match self {
            WowView::Unknown => true,
            WowView::Hidden => false,
            WowView::Shown { covers } => match overlay {
                Some(overlay) => !covers.iter().any(|cover| overlap(*cover, overlay)),
                None => true,
            },
        }
    }
}

/// Gives the keyboard back to the game after the reply box closes, so the reply can be
/// pasted at once. This only brings the game's window to the front; nothing is typed.
pub fn activate_wow() {
    #[cfg(target_os = "macos")]
    {
        if let Ok(mut open) = std::process::Command::new("open").args(["-b", "com.blizzard.worldofwarcraft"]).spawn() {
            // Waited for, so no finished process is left behind for each call.
            std::thread::spawn(move || {
                let _ = open.wait();
            });
        }
    }
}

/// Whether the companion is the app in front, the one with the keyboard. Only known on
/// macOS and on the main thread.
pub fn app_is_active() -> Option<bool> {
    #[cfg(target_os = "macos")]
    {
        let main_thread = objc2::MainThreadMarker::new()?;
        Some(objc2_app_kit::NSApplication::sharedApplication(main_thread).isActive())
    }
    #[cfg(not(target_os = "macos"))]
    None
}

fn overlap(a: Rect, b: Rect) -> bool {
    a[0] < b[0] + b[2] && b[0] < a[0] + a[2] && a[1] < b[1] + b[3] && b[1] < a[1] + a[3]
}

/// Matches the WoW client by window title or process: Wow.exe, WowClassic.exe,
/// WowClassicB.exe, or "World of Warcraft" on macOS and under Wine.
pub fn is_wow(title: &str, app: &str) -> bool {
    let app = app.to_ascii_lowercase();
    let stem = app.rsplit(['/', '\\']).next().unwrap_or(&app);
    title.trim().eq_ignore_ascii_case("World of Warcraft")
        || app.contains("world of warcraft")
        || stem.starts_with("wow")
}

/// Works out the view of WoW from the windows on screen, listed front to back.
/// Only ordinary windows count as covering it: the menu bar, the Dock and floating
/// windows of other tools sit in higher layers and are always in front of everything.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn view(front_to_back: &[Window]) -> WowView {
    let mut covers = Vec::new();
    for window in front_to_back.iter().filter(|w| w.layer == 0 && w.rect[2] > 0.0 && w.rect[3] > 0.0) {
        if is_wow(&window.title, &window.app) {
            return WowView::Shown { covers };
        }
        covers.push(window.rect);
    }
    WowView::Hidden
}

/// The windows on screen, front to back, or None when they cannot be listed.
#[cfg(target_os = "macos")]
fn windows() -> Option<Vec<Window>> {
    use objc2_core_foundation::{CFDictionary, CFNumber, CFNumberType, CFString, CGRect};
    use objc2_core_graphics::{
        CGRectMakeWithDictionaryRepresentation, CGWindowListCopyWindowInfo, CGWindowListOption,
    };
    use std::ffi::c_void;

    // SAFETY: the keys are looked up in dictionaries the window server just returned, and
    // every value is checked for null and read as the type Apple documents for its key.
    unsafe {
        let value = |dictionary: &CFDictionary, key: &str| -> Option<*const c_void> {
            let key = CFString::from_str(key);
            let value = dictionary.value((key.as_ref() as *const CFString).cast());
            (!value.is_null()).then_some(value)
        };
        let text = |dictionary: &CFDictionary, key: &str| {
            value(dictionary, key).map(|v| (*(v as *const CFString)).to_string()).unwrap_or_default()
        };
        let options = CGWindowListOption::OptionOnScreenOnly | CGWindowListOption::ExcludeDesktopElements;
        let list = CGWindowListCopyWindowInfo(options, 0)?;
        let mut windows = Vec::new();
        for i in 0..list.count() {
            let dictionary = list.value_at_index(i) as *const CFDictionary;
            if dictionary.is_null() {
                continue;
            }
            let dictionary = &*dictionary;
            let mut layer: i32 = 0;
            let Some(number) = value(dictionary, "kCGWindowLayer") else { continue };
            if !(*(number as *const CFNumber)).value(CFNumberType::IntType, (&raw mut layer).cast()) {
                continue;
            }
            let Some(bounds) = value(dictionary, "kCGWindowBounds") else { continue };
            let mut rect = CGRect::default();
            if !CGRectMakeWithDictionaryRepresentation(Some(&*(bounds as *const CFDictionary)), &mut rect) {
                continue;
            }
            windows.push(Window {
                app: text(dictionary, "kCGWindowOwnerName"),
                title: text(dictionary, "kCGWindowName"),
                layer,
                rect: [rect.origin.x as f32, rect.origin.y as f32, rect.size.width as f32, rect.size.height as f32],
            });
        }
        Some(windows)
    }
}

#[cfg(target_os = "macos")]
fn wow_view() -> WowView {
    windows().map_or(WowView::Unknown, |windows| view(&windows))
}

/// Elsewhere only the focused window is known: WoW counts as shown while it has focus.
/// The list leaves out the companion's own windows, so while one of them has focus the
/// view is Unknown and the overlay stays.
#[cfg(not(target_os = "macos"))]
fn wow_view() -> WowView {
    let focused = xcap::Window::all().ok().and_then(|windows| {
        let focused = windows.iter().find(|w| w.is_focused().unwrap_or(false))?;
        // The title alone settles it for WoW. The program's name is asked for only when it
        // does not: on Windows that fails for a program run as administrator, such as Task
        // Manager, and xcap logs an error each time.
        let title = focused.title().unwrap_or_default();
        Some(is_wow(&title, "") || is_wow(&title, &focused.app_name().unwrap_or_default()))
    });
    match focused {
        None => WowView::Unknown,
        Some(true) => WowView::Shown { covers: Vec::new() },
        Some(false) => WowView::Hidden,
    }
}

/// Polls the windows on screen and reports changes, so the overlay can hide while WoW is
/// out of sight.
pub fn spawn(ui: UiSender) {
    thread::spawn(move || {
        let mut last = None;
        loop {
            let now = wow_view();
            if last.as_ref() != Some(&now) {
                ui.send(UiEvent::WowView(now.clone()));
                last = Some(now);
            }
            thread::sleep(Duration::from_millis(250));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(app: &str, layer: i32, rect: Rect) -> Window {
        Window { app: app.into(), title: String::new(), layer, rect }
    }

    #[test]
    fn recognizes_wow_clients() {
        assert!(is_wow("World of Warcraft", "Wow.exe"));
        assert!(is_wow("", "WowClassic.exe"));
        assert!(is_wow("", r"C:\Games\World of Warcraft\_classic_beta_\WowClassicB.exe"));
        assert!(is_wow("World of Warcraft", "wine64-preloader"));
        assert!(is_wow("", "Wow"));
        assert!(!is_wow("Translate", "translate-companion"));
        assert!(!is_wow("Discord", "Discord.exe"));
    }

    #[test]
    fn overlay_shows_unless_a_window_in_front_of_wow_covers_it() {
        let overlay = Some([100.0, 600.0, 500.0, 250.0]);
        let wow = window("Wow", 0, [0.0, 30.0, 1920.0, 1050.0]);
        let menu_bar = window("Window Server", 24, [0.0, 0.0, 1920.0, 30.0]);
        let own_overlay = window("translate-companion", 3, [100.0, 600.0, 500.0, 250.0]);
        let floating_tool = window("Dictation", 1000, [0.0, 0.0, 1920.0, 1080.0]);
        let above = [menu_bar, floating_tool, own_overlay];
        let with = |front: &[Window], behind: &[Window]| {
            view(&[&above[..], front, std::slice::from_ref(&wow), behind].concat())
        };

        // WoW in front, or behind only system and always-on-top windows.
        assert!(with(&[], &[]).shows_overlay(overlay));
        // Another program has focus on a second screen.
        assert!(with(&[window("Firefox", 0, [1920.0, -520.0, 1728.0, 1084.0])], &[]).shows_overlay(overlay));
        // A small window over another part of WoW.
        assert!(with(&[window("Notes", 0, [1200.0, 100.0, 400.0, 300.0])], &[]).shows_overlay(overlay));
        // A window over the overlay's area.
        assert!(!with(&[window("Firefox", 0, [0.0, 30.0, 1920.0, 1050.0])], &[]).shows_overlay(overlay));
        assert!(!with(&[window("Notes", 0, [550.0, 800.0, 300.0, 300.0])], &[]).shows_overlay(overlay));
        // Windows behind WoW never matter.
        assert!(with(&[], &[window("Firefox", 0, [0.0, 30.0, 1920.0, 1050.0])]).shows_overlay(overlay));
        // An overlay that has not been placed yet shows while WoW does.
        assert!(with(&[window("Firefox", 0, [0.0, 30.0, 1920.0, 1050.0])], &[]).shows_overlay(None));
    }

    #[test]
    fn overlay_hides_without_wow_and_stays_when_windows_are_unknown() {
        let overlay = Some([100.0, 600.0, 500.0, 250.0]);
        let others = [window("Firefox", 0, [0.0, 30.0, 1920.0, 1050.0])];
        assert_eq!(view(&others), WowView::Hidden);
        assert!(!view(&others).shows_overlay(overlay));
        assert!(!view(&[]).shows_overlay(overlay));
        assert!(WowView::Unknown.shows_overlay(overlay));
    }
}
