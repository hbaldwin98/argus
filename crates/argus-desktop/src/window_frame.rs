//! The window's frame: the system's title bar is put away and the page
//! draws its own strip across the top, so the sidebar runs to the window's
//! edge. These are the calls that strip makes to move and size the window.
//!
//! macOS keeps its traffic lights over the page (the page leaves room for
//! them); Windows and Linux get the page's own buttons. On Windows 11 the
//! frame keeps the system's shadow, rounded corners and edge resizing.

use tauri::{Theme, WebviewWindow, WebviewWindowBuilder, Wry};

/// Takes the system's title bar off a window about to be built.
pub fn frameless<'a>(
    builder: WebviewWindowBuilder<'a, Wry, tauri::App>,
) -> WebviewWindowBuilder<'a, Wry, tauri::App> {
    #[cfg(target_os = "macos")]
    return builder.title_bar_style(tauri::TitleBarStyle::Overlay).hidden_title(true);
    #[cfg(not(target_os = "macos"))]
    builder.decorations(false)
}

/// Tints the one-pixel border Windows 11 draws round a frameless window to
/// the sidebar's colour; called again when the system's theme changes.
pub fn paint(window: &WebviewWindow) {
    let theme = window.theme().unwrap_or(Theme::Dark);
    #[cfg(windows)]
    if let Ok(hwnd) = window.hwnd() {
        dwm::border(hwnd.0, border(theme));
    }
    #[cfg(not(windows))]
    let _ = theme;
}

/// `--line` from `assets/style.css`, per theme: the rule beside the sidebar.
fn border(theme: Theme) -> u32 {
    match theme {
        Theme::Light => 0xccd0da,
        _ => 0x313244,
    }
}

#[tauri::command]
pub async fn minimize(window: WebviewWindow) -> Result<(), String> {
    window.minimize().map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn toggle_maximize(window: WebviewWindow) -> Result<(), String> {
    let maximized = window.is_maximized().map_err(|err| err.to_string())?;
    if maximized { window.unmaximize() } else { window.maximize() }.map_err(|err| err.to_string())
}

/// Closing the window ends the program, as the system's button would.
#[tauri::command]
pub async fn close_window(window: WebviewWindow) -> Result<(), String> {
    window.close().map_err(|err| err.to_string())
}

/// A press on the strip's empty space: the system moves the window from
/// here until the button is let go, snapping it to an edge as usual.
#[tauri::command]
pub async fn drag_window(window: WebviewWindow) -> Result<(), String> {
    window.start_dragging().map_err(|err| err.to_string())
}

/// Windows 11's snap layouts, which open over the system's maximize
/// button on hover. A page's button is not the system's, so the hover asks
/// for them by their key, Win-Z, which opens them by the window's corner.
#[tauri::command]
pub async fn snap_layouts(window: WebviewWindow) -> Result<(), String> {
    #[cfg(windows)]
    {
        window.set_focus().map_err(|err| err.to_string())?;
        dwm::press_win_z();
    }
    #[cfg(not(windows))]
    let _ = window;
    Ok(())
}

#[cfg(windows)]
mod dwm {
    use std::ffi::c_void;

    use windows_sys::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_BORDER_COLOR};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VK_LWIN, VK_Z,
    };

    pub fn border(hwnd: *mut c_void, rgb: u32) {
        let colorref = super::colorref(rgb);
        // A failure is Windows 10, which draws no coloured border.
        unsafe {
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_BORDER_COLOR as u32,
                &colorref as *const u32 as *const c_void,
                std::mem::size_of::<u32>() as u32,
            );
        }
    }

    pub fn press_win_z() {
        let key = |vk: u16, up: bool| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: if up { KEYEVENTF_KEYUP } else { 0 },
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        let inputs = [key(VK_LWIN, false), key(VK_Z, false), key(VK_Z, true), key(VK_LWIN, true)];
        unsafe {
            SendInput(inputs.len() as u32, inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32);
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
        assert_eq!(colorref(0x313244), 0x443231);
    }

    #[test]
    fn the_border_takes_the_line_colour_the_stylesheet_names() {
        let css = include_str!("../assets/style.css");
        assert!(css.contains(&format!("--surface0: #{:06x};", border(Theme::Dark))));
        assert!(css.contains(&format!("--surface0: #{:06x};", border(Theme::Light))));
    }
}
