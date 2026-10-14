//! The window's title bar: on Windows 11 it is painted the page's chrome
//! colour, so the native bar reads as the top of the app rather than a
//! grey strip above it.
//!
//! The bar stays the system's own — its buttons, snap layouts and drag —
//! and is only tinted. Windows 10 ignores the attributes and keeps its bar.

use tauri::{Theme, WebviewWindow};

/// `--bg-chrome` and `--fg` from `assets/style.css`, per theme. The page
/// follows the system's scheme and so does this, so the two agree.
fn colours(theme: Theme) -> (u32, u32) {
    match theme {
        Theme::Light => (0xe6e9ef, 0x4c4f69),
        _ => (0x181825, 0xcdd6f4),
    }
}

/// Tints the bar for the window's current theme; called again when the
/// system's theme changes.
pub fn paint(window: &WebviewWindow) {
    let theme = window.theme().unwrap_or(Theme::Dark);
    let (background, text) = colours(theme);
    #[cfg(windows)]
    if let Ok(hwnd) = window.hwnd() {
        dwm::paint(hwnd.0, background, text);
    }
    #[cfg(not(windows))]
    let _ = (window, background, text);
}

#[cfg(windows)]
mod dwm {
    use std::ffi::c_void;

    use windows_sys::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR,
    };

    pub fn paint(hwnd: *mut c_void, background: u32, text: u32) {
        set(hwnd, DWMWA_CAPTION_COLOR, background);
        set(hwnd, DWMWA_BORDER_COLOR, background);
        set(hwnd, DWMWA_TEXT_COLOR, text);
    }

    fn set(hwnd: *mut c_void, attribute: i32, rgb: u32) {
        let colorref = super::colorref(rgb);
        // A failure is Windows 10, which keeps its own bar; nothing to report.
        unsafe {
            DwmSetWindowAttribute(
                hwnd,
                attribute as u32,
                &colorref as *const u32 as *const c_void,
                std::mem::size_of::<u32>() as u32,
            );
        }
    }
}

/// A COLORREF is `0x00BBGGRR`, the reverse of the stylesheet's `#rrggbb`.
#[cfg_attr(not(windows), allow(dead_code))]
fn colorref(rgb: u32) -> u32 {
    (rgb & 0xff) << 16 | (rgb & 0xff00) | (rgb >> 16 & 0xff)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_colorref_reverses_the_stylesheets_byte_order() {
        assert_eq!(colorref(0x181825), 0x251818);
    }

    #[test]
    fn the_bar_takes_the_chrome_colour_the_stylesheet_names() {
        let css = include_str!("../assets/style.css");
        for theme in [Theme::Dark, Theme::Light] {
            let (background, text) = colours(theme);
            assert!(css.contains(&format!("--mantle: #{background:06x};")), "{theme:?}");
            assert!(css.contains(&format!("--text: #{text:06x};")), "{theme:?}");
        }
    }
}
