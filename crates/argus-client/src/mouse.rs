use argus_protocol::{MouseEncoding, MouseTracking};
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

/// Encode a mouse event as the mouse-tracking sequence the child asked for,
/// in coordinates relative to `area`, for forwarding into a pane's pty.
///
/// Returns `None` when the event must not be forwarded at all: outside
/// `area`, or of a kind this child's mode does not report — including the
/// common case of a child that asked for no mouse reporting whatsoever.
/// That last one is the whole reason `tracking` is a parameter: a pty is
/// not a terminal that quietly drops what it did not enable, so an
/// unrequested `ESC [ < 65 ; 40 ; 12 M` is simply typed into whatever the
/// child was prompting for.
pub fn encode_mouse(ev: &MouseEvent, area: Rect, tracking: MouseTracking) -> Option<Vec<u8>> {
    if !tracking.enabled() || area.width == 0 || area.height == 0 {
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

    let sgr = tracking.encoding == MouseEncoding::Sgr;
    let (cb, press) = match ev.kind {
        MouseEventKind::Down(btn) => (button_code(btn), true),
        // Without SGR there is no way to say which button came up, so the
        // release code is the same "3" for all of them.
        MouseEventKind::Up(btn) if tracking.wants_release() => {
            (if sgr { button_code(btn) } else { 3 }, false)
        }
        MouseEventKind::Up(_) => return None,
        MouseEventKind::Drag(btn) if tracking.wants_drag() => (button_code(btn) + 32, true),
        MouseEventKind::Drag(_) => return None,
        MouseEventKind::Moved if tracking.wants_bare_motion() => (3 + 32, true),
        MouseEventKind::Moved => return None,
        MouseEventKind::ScrollUp => (64, true),
        MouseEventKind::ScrollDown => (65, true),
        MouseEventKind::ScrollLeft => (66, true),
        MouseEventKind::ScrollRight => (67, true),
    };

    match tracking.encoding {
        MouseEncoding::Sgr => Some(
            format!(
                "\x1b[<{};{};{}{}",
                cb,
                x + 1,
                y + 1,
                if press { 'M' } else { 'm' }
            )
            .into_bytes(),
        ),
        MouseEncoding::Utf8 => {
            let mut out = b"\x1b[M".to_vec();
            let mut push = |v: u16| {
                let mut buf = [0u8; 4];
                out.extend_from_slice(
                    char::from_u32(32 + v as u32)
                        .unwrap()
                        .encode_utf8(&mut buf)
                        .as_bytes(),
                );
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
            Some(vec![
                0x1b,
                b'[',
                b'M',
                32 + cb,
                32 + (x + 1) as u8,
                32 + (y + 1) as u8,
            ])
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
    use argus_protocol::MouseMode;
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
    fn the_original_encoding_offsets_every_field_by_32() {
        let bytes = encode_mouse(
            &ev(MouseEventKind::ScrollDown, 0, 0),
            Rect::new(0, 0, 80, 24),
            tracking(MouseMode::PressRelease, MouseEncoding::Default),
        )
        .unwrap();
        assert_eq!(bytes, vec![0x1b, b'[', b'M', 32 + 65, 33, 33]);
    }

    #[test]
    fn a_release_without_sgr_cannot_name_its_button() {
        let bytes = encode_mouse(
            &ev(MouseEventKind::Up(MouseButton::Right), 0, 0),
            Rect::new(0, 0, 80, 24),
            tracking(MouseMode::PressRelease, MouseEncoding::Default),
        )
        .unwrap();
        assert_eq!(bytes[3], 32 + 3);
    }

    #[test]
    fn a_drag_is_only_reported_to_a_child_tracking_motion() {
        let area = Rect::new(0, 0, 80, 24);
        let drag = ev(MouseEventKind::Drag(MouseButton::Left), 1, 1);
        assert_eq!(
            encode_mouse(
                &drag,
                area,
                tracking(MouseMode::PressRelease, MouseEncoding::Sgr)
            ),
            None
        );
        assert!(encode_mouse(
            &drag,
            area,
            tracking(MouseMode::ButtonMotion, MouseEncoding::Sgr)
        )
        .is_some());
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
