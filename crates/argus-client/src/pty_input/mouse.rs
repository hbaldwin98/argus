//! A terminal mouse event as the protocol's, relative to the pane's area:
//! the sequence itself is `argus_protocol::mouse`'s to decide.

use argus_protocol::mouse::{self, MouseButton as Button, MouseKind};
use argus_protocol::MouseTracking;
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

/// Encode a mouse event as the mouse-tracking sequence the child asked for,
/// in coordinates relative to `area`, for forwarding into a pane's pty.
/// `None` outside `area`, and for whatever the child's mode does not report.
pub fn encode_mouse(ev: &MouseEvent, area: Rect, tracking: MouseTracking) -> Option<Vec<u8>> {
    if ev.column < area.x || ev.row < area.y {
        return None;
    }
    let (col, row) = (ev.column - area.x, ev.row - area.y);
    if col >= area.width || row >= area.height {
        return None;
    }
    let kind = match ev.kind {
        MouseEventKind::Down(btn) => MouseKind::Down(button(btn)),
        MouseEventKind::Up(btn) => MouseKind::Up(button(btn)),
        MouseEventKind::Drag(btn) => MouseKind::Drag(button(btn)),
        MouseEventKind::Moved => MouseKind::Moved,
        MouseEventKind::ScrollUp => MouseKind::ScrollUp,
        MouseEventKind::ScrollDown => MouseKind::ScrollDown,
        MouseEventKind::ScrollLeft => MouseKind::ScrollLeft,
        MouseEventKind::ScrollRight => MouseKind::ScrollRight,
    };
    mouse::encode(&mouse::MouseEvent { kind, col, row }, tracking)
}

fn button(btn: MouseButton) -> Button {
    match btn {
        MouseButton::Left => Button::Left,
        MouseButton::Middle => Button::Middle,
        MouseButton::Right => Button::Right,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_protocol::{MouseEncoding, MouseMode};
    use crossterm::event::KeyModifiers;

    fn tracking(mode: MouseMode, encoding: MouseEncoding) -> MouseTracking {
        MouseTracking { mode, encoding }
    }

    fn ev(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    #[test]
    fn a_child_that_asked_for_nothing_gets_nothing() {
        let area = Rect::new(0, 0, 80, 24);
        for kind in [
            MouseEventKind::Down(MouseButton::Left),
            MouseEventKind::Up(MouseButton::Left),
            MouseEventKind::ScrollDown,
        ] {
            assert_eq!(
                encode_mouse(&ev(kind, 4, 2), area, MouseTracking::default()),
                None
            );
        }
    }

    #[test]
    fn sgr_reports_are_relative_to_the_area_and_one_based() {
        let bytes = encode_mouse(
            &ev(MouseEventKind::Down(MouseButton::Left), 14, 7),
            Rect::new(10, 5, 20, 10),
            tracking(MouseMode::PressRelease, MouseEncoding::Sgr),
        )
        .unwrap();
        assert_eq!(bytes, b"\x1b[<0;5;3M");
    }

    #[test]
    fn events_outside_the_area_are_not_reported() {
        let area = Rect::new(10, 5, 4, 4);
        let t = tracking(MouseMode::PressRelease, MouseEncoding::Sgr);
        for (x, y) in [(9, 6), (10, 4), (14, 6), (10, 9)] {
            assert_eq!(
                encode_mouse(&ev(MouseEventKind::Down(MouseButton::Left), x, y), area, t),
                None
            );
        }
    }
}
