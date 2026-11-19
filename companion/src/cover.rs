//! The strip cover: a small window over the pixel strip, so the player sees a plain dark
//! corner instead of flickering colors. Windows can leave a window out of screen captures
//! while still showing it (`WDA_EXCLUDEFROMCAPTURE`, Windows 10 version 2004 and later),
//! so the companion's own capture still sees the strip underneath. The cover is put over
//! the strip only once that has been set: a cover that captures saw would hide the strip
//! from the companion too.

/// The cover window's title, for finding it among the windows.
pub const TITLE: &str = "Nai Ma Speak strip cover";

/// Whether this system can cover the strip.
pub const AVAILABLE: bool = cfg!(windows);

/// Whether the cover window is left out of screen captures, leaving it out first if it is
/// not yet. Checked on every pass, not once: should the window ever be made anew, the new
/// one would show in captures. False when it is not found or the system refuses; the
/// cover must then stay off the strip.
#[cfg(windows)]
pub fn hide_from_capture() -> bool {
    use std::ffi::c_void;
    #[link(name = "user32")]
    extern "system" {
        fn FindWindowW(class: *const u16, name: *const u16) -> *mut c_void;
        fn GetWindowDisplayAffinity(window: *mut c_void, affinity: *mut u32) -> i32;
        fn SetWindowDisplayAffinity(window: *mut c_void, affinity: u32) -> i32;
    }
    const WDA_EXCLUDEFROMCAPTURE: u32 = 0x11;
    let title: Vec<u16> = TITLE.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: the title is a NUL-terminated UTF-16 string that outlives the calls, the
    // window handle is checked for null before use, and `current` is a valid u32 to write.
    unsafe {
        let window = FindWindowW(std::ptr::null(), title.as_ptr());
        if window.is_null() {
            return false;
        }
        let mut current = 0;
        if GetWindowDisplayAffinity(window, &mut current) != 0 && current == WDA_EXCLUDEFROMCAPTURE {
            return true;
        }
        SetWindowDisplayAffinity(window, WDA_EXCLUDEFROMCAPTURE) != 0
    }
}

#[cfg(not(windows))]
pub fn hide_from_capture() -> bool {
    false
}
