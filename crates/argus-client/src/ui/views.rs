//! The tab strip, and the views that are not the spine.
//!
//! The strip is one row at the very top of the page, above every card and
//! every overlay's frame. It costs a row on purpose: a view you cannot see
//! the existence of is a view nobody opens, and the numbers on the tabs
//! are the whole of their documentation.

use super::*;

use crate::app::View;

/// The shortest frame that gets a strip. Below it the page has no top
/// gutter to draw one in, and the digits still work.
const MIN_HEIGHT_FOR_STRIP: u16 = 3;

/// Where the tab strip goes: the blank row the page is already inset by,
/// so the strip costs the views underneath nothing at all.
///
/// A row taken off the page instead would come out of whatever is open —
/// and the settings panel is already as short as it can be drawn.
pub(super) fn strip_row(frame: Rect, page: Rect) -> Option<Rect> {
    if frame.height < MIN_HEIGHT_FOR_STRIP {
        return None;
    }
    Some(Rect {
        x: page.x,
        y: frame.y,
        width: page.width,
        height: 1,
    })
}

/// One tab's text, including the padding that makes it a click target
/// rather than a word.
fn tab_text(view: View) -> String {
    format!(" {} {} ", view.digit(), view.label())
}

/// Which tab a point falls on. Shared with the renderer rather than
/// re-derived, so a click lands on the tab that was actually drawn.
pub fn tab_at(strip: Rect, x: u16, y: u16) -> Option<View> {
    // A zero-sized strip is one that was not drawn — on a short terminal,
    // or before the first frame — and its default rect sits on row 0,
    // where a click would otherwise land on a tab that is not there.
    if strip.width == 0 || strip.height == 0 {
        return None;
    }
    if y != strip.y || x < strip.x {
        return None;
    }
    let mut cell = strip.x;
    for view in View::ALL {
        let width = tab_text(view).chars().count() as u16;
        if x >= cell && x < cell + width {
            return Some(view);
        }
        cell += width;
    }
    None
}

pub(super) fn render_view_tabs(f: &mut Frame, app: &mut App, area: Rect, th: Theme) {
    let mut spans = Vec::new();
    for view in View::ALL {
        let open = view == app.view;
        // The open tab is the only accented thing on the row, and it is
        // the elevation that says which one it is — the same trick the
        // cards use, one row tall.
        let style = if open {
            Style::default()
                .fg(th.accent)
                .bg(th.surface_focus)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(th.dim).bg(th.bg)
        };
        spans.push(Span::styled(tab_text(view), style));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
    app.layout.views = Panel {
        outer: area,
        inner: area,
        first: 0,
    };
}

/// The decision board.
///
/// Empty until agents can write to it (ROADMAP.md, "P6.5"). It says what
/// it is for rather than nothing at all, because a blank card on a tab
/// somebody just pressed reads as a bug.
pub(super) fn render_decisions(f: &mut Frame, app: &mut App, area: Rect, th: Theme) {
    let title = match app.current_project() {
        Some(p) => format!("decisions · {}", p.name),
        None => "decisions".to_string(),
    };
    let block = panel_block(&title, true, th, area.width);
    let inner = block.inner(area);
    f.render_widget(block, area);
    f.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                "no decisions recorded yet",
                Style::default().fg(th.text),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Agents write here as they choose between options: what was chosen, what \
                 it was chosen over, and what forced it. Each one hangs off the decision \
                 that constrained it, so what accumulates is a tree rather than a log.",
                Style::default().fg(th.dim),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "1 goes back to the spine.",
                Style::default().fg(th.dim),
            )),
        ])
        .wrap(Wrap { trim: true }),
        inner,
    );
    app.layout.content = Panel {
        outer: area,
        inner,
        first: 0,
    };
}
