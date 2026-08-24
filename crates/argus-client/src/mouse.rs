use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

/// Encode a mouse event as an SGR mouse-tracking sequence (`ESC [ < Cb ; Px ;
/// Py M/m`) relative to `area`, for forwarding into a pane's pty. Returns
/// `None` for events outside `area` or ones with no VT equivalent (plain
/// moves without a button held).
pub fn encode_mouse(ev: &MouseEvent, area: Rect) -> Option<Vec<u8>> {
    if area.width == 0 || area.height == 0 {
        return None;
    }
    if ev.column < area.x || ev.row < area.y {
        return None;
    }
    let x = ev.column - area.x;
    let y = ev.row - area.y;
    if x >= area.width || y >= area.height {
        return None;
    }

    let (cb, suffix) = match ev.kind {
        MouseEventKind::Down(btn) => (button_code(btn), 'M'),
        MouseEventKind::Up(btn) => (button_code(btn), 'm'),
        MouseEventKind::Drag(btn) => (button_code(btn) + 32, 'M'),
        MouseEventKind::ScrollUp => (64, 'M'),
        MouseEventKind::ScrollDown => (65, 'M'),
        MouseEventKind::ScrollLeft => (66, 'M'),
        MouseEventKind::ScrollRight => (67, 'M'),
        MouseEventKind::Moved => return None,
    };

    Some(format!("\x1b[<{};{};{}{}", cb, x + 1, y + 1, suffix).into_bytes())
}

fn button_code(btn: MouseButton) -> u8 {
    match btn {
        MouseButton::Left => 0,
        MouseButton::Middle => 1,
        MouseButton::Right => 2,
    }
}
