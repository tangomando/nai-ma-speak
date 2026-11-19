//! The chat windows of the game, as the addon reports them (see `protocol`), so the
//! overlay can sit on one of them.

use crate::focus::Rect;

/// One chat window. Lengths are in game pixels from the top-left corner of the game's
/// picture when parsed, and in screen points after `on_screen`.
#[derive(Debug, Clone, PartialEq)]
pub struct ChatWindow {
    /// The name on the window's tab.
    pub name: String,
    /// False while another tab of the same dock is selected, or the interface is hidden.
    pub shown: bool,
    pub rect: Rect,
    pub font_size: f32,
}

/// Where the game's picture is on screen, worked out from the strip, which the addon
/// draws at the picture's top-left corner with blocks of `strip::BLOCK` game pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Picture {
    /// The monitor's top-left corner in screen points.
    pub monitor: (f32, f32),
    /// Captured pixels per screen point on that monitor.
    pub scale: f32,
    /// The strip's top-left corner in captured pixels, and its block size.
    pub strip: (f32, f32),
    pub block: f32,
}

impl Picture {
    /// Converts a window from game pixels to screen points.
    pub fn on_screen(&self, window: &ChatWindow) -> ChatWindow {
        let captured = self.block / crate::strip::BLOCK as f32; // captured pixels per game pixel
        let points = captured / self.scale;
        let [x, y, width, height] = window.rect;
        ChatWindow {
            rect: [
                self.monitor.0 + (self.strip.0 + x * captured) / self.scale,
                self.monitor.1 + (self.strip.1 + y * captured) / self.scale,
                width * points,
                height * points,
            ],
            font_size: window.font_size * points,
            ..window.clone()
        }
    }
}

/// The window the overlay is anchored to: the one whose tab is named `tab`, ignoring case
/// and surrounding spaces. None when no name is set or no window has it.
pub fn anchor<'a>(windows: &'a [ChatWindow], tab: &str) -> Option<&'a ChatWindow> {
    let tab = tab.trim();
    if tab.is_empty() {
        return None;
    }
    windows.iter().find(|window| window.name.trim().to_lowercase() == tab.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::tests::windows;

    #[test]
    fn places_a_window_on_screen() {
        let window = &windows()[0];
        // A plain display: the game fills a window whose picture starts at 76,30.
        let plain = Picture { monitor: (0.0, 0.0), scale: 1.0, strip: (76.0, 30.0), block: 2.0 };
        assert_eq!(plain.on_screen(window).rect, [109.0, 790.0, 440.0, 180.0]);
        assert_eq!(plain.on_screen(window).font_size, 14.0);
        // A HiDPI display to the right: captures have two pixels per point.
        let hidpi = Picture { monitor: (1920.0, -554.0), scale: 2.0, strip: (0.0, 66.0), block: 2.0 };
        assert_eq!(hidpi.on_screen(window).rect, [1936.5, -141.0, 220.0, 90.0]);
        assert_eq!(hidpi.on_screen(window).font_size, 7.0);
    }

    #[test]
    fn finds_the_anchor_by_tab_name() {
        let windows = windows();
        assert_eq!(anchor(&windows, " general ").map(|w| w.shown), Some(true));
        assert_eq!(anchor(&windows, "翻譯").map(|w| w.shown), Some(false));
        assert_eq!(anchor(&windows, "Trade"), None);
        assert_eq!(anchor(&windows, "  "), None);
    }
}
