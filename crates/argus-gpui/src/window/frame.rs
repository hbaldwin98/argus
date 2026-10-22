//! What the platform window itself is told about the look: the colour of the border Windows 11
//! draws round a frameless window, and the dark or light tint Mica takes.
//!
//! GPUI has a call for what shows through (`set_background_appearance`) and none for these two,
//! so on Windows they are set on the window's handle with DWM, as the Tauri window does
//! (`argus-desktop`'s `window_frame`). Windows 10 answers an error and draws no coloured border,
//! which is no fault; every other system has nothing to set (macOS's frame and Linux's edges
//! are the platform's own).

use gpui_kit::Window;

/// Puts `border` (`0xrrggbb`) round the window and tints Mica for `dark`.
pub fn paint(window: &Window, border: u32, dark: bool) {
    #[cfg(windows)]
    dwm::paint(window, border, dark);
    #[cfg(not(windows))]
    let _ = (window, border, dark);
}

/// A COLORREF is `0x00BBGGRR`, the reverse of the stylesheet's `#rrggbb`.
#[cfg_attr(not(windows), allow(dead_code))]
fn colorref(rgb: u32) -> u32 {
    (rgb & 0xff) << 16 | (rgb & 0xff00) | (rgb >> 16 & 0xff)
}

#[cfg(windows)]
mod dwm {
    use std::ffi::c_void;

    use gpui_kit::Window;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    const DWMWA_USE_IMMERSIVE_DARK_MODE: u32 = 20;
    const DWMWA_BORDER_COLOR: u32 = 34;

    #[link(name = "dwmapi")]
    extern "system" {
        fn DwmSetWindowAttribute(hwnd: *mut c_void, attribute: u32, value: *const c_void, size: u32) -> i32;
    }

    pub fn paint(window: &Window, border: u32, dark: bool) {
        let Ok(handle) = HasWindowHandle::window_handle(window) else { return };
        let RawWindowHandle::Win32(win32) = handle.as_raw() else { return };
        let hwnd = win32.hwnd.get() as *mut c_void;
        let (colour, dark) = (super::colorref(border), u32::from(dark));
        // A failure is Windows 10, which draws no coloured border.
        unsafe {
            DwmSetWindowAttribute(hwnd, DWMWA_BORDER_COLOR, &colour as *const u32 as *const c_void, 4);
            DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &dark as *const u32 as *const c_void, 4);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_colorref_reverses_the_stylesheets_byte_order() {
        assert_eq!(colorref(0x313244), 0x443231);
    }
}
