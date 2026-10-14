//! The bytes a mouse event is to a pane's child: the tracking sequence it
//! asked for, or nothing when it asked for none, since a pty does not
//! quietly discard what it did not enable. Decided once so every client
//! reports the mouse the same way.

use crate::{MouseEncoding, MouseTracking};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseKind {
    Down(MouseButton),
    Up(MouseButton),
    Drag(MouseButton),
    Moved,
    ScrollUp,
    ScrollDown,
    ScrollLeft,
    ScrollRight,
}

/// A mouse event at a cell of the pane, counted from its top-left at 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseEvent {
    pub kind: MouseKind,
    pub col: u16,
    pub row: u16,
}

/// The sequence `tracking` asks for, or `None` when the event must not be
/// sent: of a kind this child's mode does not report — including the
/// common case of a child that asked for no mouse reporting whatsoever.
/// An unrequested `ESC [ < 65 ; 40 ; 12 M` is simply typed into whatever
/// the child was prompting for.
pub fn encode(ev: &MouseEvent, tracking: MouseTracking) -> Option<Vec<u8>> {
    if !tracking.enabled() {
        return None;
    }
    let (x, y) = (ev.col, ev.row);
    let sgr = tracking.encoding == MouseEncoding::Sgr;
    let (cb, press) = match ev.kind {
        MouseKind::Down(btn) => (button_code(btn), true),
        // Without SGR there is no way to say which button came up, so the
        // release code is the same "3" for all of them.
        MouseKind::Up(btn) if tracking.wants_release() => (if sgr { button_code(btn) } else { 3 }, false),
        MouseKind::Up(_) => return None,
        MouseKind::Drag(btn) if tracking.wants_drag() => (button_code(btn) + 32, true),
        MouseKind::Drag(_) => return None,
        MouseKind::Moved if tracking.wants_bare_motion() => (3 + 32, true),
        MouseKind::Moved => return None,
        MouseKind::ScrollUp => (64, true),
        MouseKind::ScrollDown => (65, true),
        MouseKind::ScrollLeft => (66, true),
        MouseKind::ScrollRight => (67, true),
    };

    match tracking.encoding {
        MouseEncoding::Sgr => {
            Some(format!("\x1b[<{};{};{}{}", cb, x + 1, y + 1, if press { 'M' } else { 'm' }).into_bytes())
        }
        MouseEncoding::Utf8 => {
            let mut out = b"\x1b[M".to_vec();
            let mut push = |v: u16| {
                let mut buf = [0u8; 4];
                let ch = char::from_u32(32 + v as u32).unwrap_or(' ');
                out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
            };
            push(cb as u16);
            push(x + 1);
            push(y + 1);
            Some(out)
        }
        // The original encoding is one byte per field, so a coordinate past
        // 223 has no representation. Dropping it beats sending a report
        // that points at the wrong cell.
        MouseEncoding::Default => {
            if x + 1 > 223 || y + 1 > 223 {
                return None;
            }
            Some(vec![0x1b, b'[', b'M', 32 + cb, 32 + (x + 1) as u8, 32 + (y + 1) as u8])
        }
    }
}

fn button_code(btn: MouseButton) -> u8 {
    match btn {
        MouseButton::Left => 0,
        MouseButton::Middle => 1,
        MouseButton::Right => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MouseMode;

    fn tracking(mode: MouseMode, encoding: MouseEncoding) -> MouseTracking {
        MouseTracking { mode, encoding }
    }

    fn ev(kind: MouseKind, col: u16, row: u16) -> MouseEvent {
        MouseEvent { kind, col, row }
    }

    #[test]
    fn a_child_that_asked_for_nothing_gets_nothing() {
        for kind in [MouseKind::Down(MouseButton::Left), MouseKind::Up(MouseButton::Left), MouseKind::ScrollDown] {
            assert_eq!(encode(&ev(kind, 4, 2), MouseTracking::default()), None);
        }
    }

    #[test]
    fn sgr_reports_are_one_based() {
        let bytes =
            encode(&ev(MouseKind::Down(MouseButton::Left), 4, 2), tracking(MouseMode::PressRelease, MouseEncoding::Sgr));
        assert_eq!(bytes.unwrap(), b"\x1b[<0;5;3M");
    }

    #[test]
    fn the_original_encoding_offsets_every_field_by_32() {
        let bytes = encode(&ev(MouseKind::ScrollDown, 0, 0), tracking(MouseMode::PressRelease, MouseEncoding::Default));
        assert_eq!(bytes.unwrap(), vec![0x1b, b'[', b'M', 32 + 65, 33, 33]);
    }

    #[test]
    fn a_release_without_sgr_cannot_name_its_button() {
        let bytes = encode(
            &ev(MouseKind::Up(MouseButton::Right), 0, 0),
            tracking(MouseMode::PressRelease, MouseEncoding::Default),
        );
        assert_eq!(bytes.unwrap()[3], 32 + 3);
    }

    #[test]
    fn a_drag_is_only_reported_to_a_child_tracking_motion() {
        let drag = ev(MouseKind::Drag(MouseButton::Left), 1, 1);
        assert_eq!(encode(&drag, tracking(MouseMode::PressRelease, MouseEncoding::Sgr)), None);
        assert!(encode(&drag, tracking(MouseMode::ButtonMotion, MouseEncoding::Sgr)).is_some());
    }

    #[test]
    fn the_original_encoding_drops_a_cell_it_cannot_name() {
        let far = ev(MouseKind::ScrollUp, 230, 0);
        assert_eq!(encode(&far, tracking(MouseMode::PressRelease, MouseEncoding::Default)), None);
        assert!(encode(&far, tracking(MouseMode::PressRelease, MouseEncoding::Sgr)).is_some());
    }
}
