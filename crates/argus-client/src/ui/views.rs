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

/// How much of a row's width the tree guides may take before the text is
/// what suffers. A decision nested past this is drawn at the last indent
/// that still leaves room to read it.
const MAX_BOARD_INDENT: usize = 24;

/// The decision board, drawn as the tree it is.
///
/// Two lines per decision, like every other list in Argus: what was
/// chosen, then the dimmer line of what it was chosen over and what forced
/// it. A superseded decision keeps its place and goes dim — the road not
/// taken is most of what a reader came for.
pub(super) fn render_decisions(f: &mut Frame, app: &mut App, area: Rect, th: Theme) {
    let title = match app.board.as_ref().map(|b| b.name.clone()) {
        Some(name) => format!("decisions · {name}"),
        None => "decisions".to_string(),
    };
    let block = panel_block(&title, true, th, area.width);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let count = app.board_rows().len();
    let per_row = ROW_HEIGHT as usize;
    let visible = (inner.height as usize) / per_row.max(1);
    // A scrolled board's top row is `first`, not row zero, and a click has
    // to resolve against the rows that were actually drawn.
    let first = scrolled_to_show(0, Some(app.board_sel), visible, count);
    app.layout.content = Panel {
        outer: area,
        inner,
        first,
    };
    if count == 0 {
        render_empty_board(f, inner, th);
        return;
    }

    let rows = app.board_rows();
    let mut lines = Vec::new();
    for (index, (depth, decision)) in rows.iter().enumerate().skip(first).take(visible) {
        let selected = index == app.board_sel;
        let indent = " ".repeat((depth * 2).min(MAX_BOARD_INDENT));
        let dim = decision.superseded();
        let name_style = match (selected, dim) {
            (_, true) => Style::default().fg(th.dim),
            (true, false) => Style::default().fg(th.text).add_modifier(Modifier::BOLD),
            (false, false) => Style::default().fg(th.text),
        };
        let mut name = vec![
            Span::styled(
                if selected { MARKER } else { GUTTER },
                Style::default().fg(th.accent),
            ),
            Span::styled(format!("{indent}#{} ", decision.id), Style::default().fg(th.dim)),
            Span::styled(decision.chose.clone(), name_style),
        ];
        if let Some(by) = decision.superseded_by {
            name.push(Span::styled(
                format!("  superseded by #{by}"),
                Style::default().fg(th.dim),
            ));
        }
        lines.push(Line::from(name));
        lines.push(Line::from(Span::styled(
            format!(" {indent}  {}", board_detail(decision)),
            Style::default().fg(th.dim),
        )));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

/// The dimmer second line: what it was chosen over, and what forced it.
/// Both are optional, and a decision with neither says so rather than
/// leaving a blank row that reads as a rendering fault.
fn board_detail(decision: &argus_protocol::Decision) -> String {
    let mut parts = Vec::new();
    if let Some(over) = &decision.over {
        parts.push(format!("over {over}"));
    }
    if let Some(because) = &decision.because {
        parts.push(format!("because {because}"));
    }
    if parts.is_empty() {
        parts.push("no alternative or reason recorded".to_string());
    }
    parts.join(" · ")
}

/// A board nobody has written to yet. It says what it is for rather than
/// nothing at all, because a blank card on a tab somebody just pressed
/// reads as a bug.
fn render_empty_board(f: &mut Frame, inner: Rect, th: Theme) {
    f.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                "no decisions recorded yet",
                Style::default().fg(th.text),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Agents record a decision when they choose between real options while \
                 planning work: what was chosen, what it was chosen over, and what \
                 forced it. Each one hangs off the decision that constrained it, so \
                 what accumulates is a reference tree for the feature rather than a log.",
                Style::default().fg(th.dim),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "r refreshes · 1 goes back to the spine",
                Style::default().fg(th.dim),
            )),
        ])
        .wrap(Wrap { trim: true }),
        inner,
    );
}
