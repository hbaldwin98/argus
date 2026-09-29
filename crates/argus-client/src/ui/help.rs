//! The keymap, as a window you ask for rather than a bar you read.
//!
//! The status bar holds about a dozen keys on a wide terminal and half that
//! on a narrow one, so it answered "what can I press here?" by not
//! answering — it showed the keys that fit and left the rest to be
//! discovered. This is the other half: `?` from anywhere that is not a
//! typing surface, which lets the bar go back to being a reminder of the
//! few keys worth having in front of you at all times. What it lists is
//! each mode's keymap, declared in `app/mode/keymap`; this only draws it.

use super::*;

/// Draws the window over `area`, which is the columns and not the status
/// bar: the bar is where it says how to put the window away.
pub(super) fn render_help(f: &mut Frame, app: &mut App, area: Rect, th: Theme) {
    // Asked of the mode rather than fixed, because `?` in a diff and `?` in
    // the columns are different questions, and answering both with one
    // wall of text answers neither.
    let blocks: Vec<Vec<Line>> = app
        .mode()
        .keymap()
        .help()
        .map(|group| block_of(group.title(), group.listed().collect(), th))
        .collect();
    let content_width = blocks.iter().flatten().map(Line::width).max().unwrap_or(0) as u16;
    let rows: usize = blocks.iter().map(Vec::len).sum();

    // Two columns only when both halves would still be wide enough to
    // read, and only when one column would not have fit anyway; a 30-cell
    // half is a worse answer than a short list.
    let two_up = area.width >= (content_width + 3) * 2 + 4 && rows as u16 + 3 > area.height;
    let columns = if two_up { 2 } else { 1 };
    let width = ((content_width + 3) * columns + 2).min(area.width);

    // Groups are dealt into the columns whole and in order: the left
    // column fills to about half the list and the rest follows in the
    // right, so it still reads top to bottom. Splitting a group across the
    // fold would put "the view" under a heading that says "moving".
    let half = rows.div_ceil(2);
    let mut left: Vec<Line> = Vec::new();
    let mut right: Vec<Line> = Vec::new();
    for lines in blocks {
        if two_up && left.len() >= half {
            right.extend(lines);
        } else {
            left.extend(lines);
        }
    }

    // The window is sized to what it turned out to hold rather than to the
    // screen: a keymap that fits in half the terminal should not cover all
    // of it, since what is behind it is what the keys act on.
    let tallest = left.len().max(right.len()) as u16;
    let height = (tallest + 3).min(area.height);
    let popup = centered_rect(width, height, area);

    f.render_widget(Clear, popup);
    // On the terminal's own background, like the shell behind it.
    let block =
        panel_block("keys · esc to close", true, th, popup.width).style(Style::default().bg(th.bg));
    let inner = block.inner(popup);
    f.render_widget(block, popup);
    app.layout.help = Panel {
        outer: popup,
        inner,
        first: 0,
    };
    if inner.height == 0 {
        return;
    }

    let (left_area, right_area) = if two_up {
        let half = inner.width / 2;
        (
            Rect {
                width: half,
                ..inner
            },
            Rect {
                x: inner.x + half,
                width: inner.width - half,
                ..inner
            },
        )
    } else {
        (inner, Rect { width: 0, ..inner })
    };

    let scroll = clamp_scroll(app, tallest as usize, inner.height as usize);
    for (lines, column) in [(left, left_area), (right, right_area)] {
        if column.width == 0 {
            continue;
        }
        let body: Vec<Line> = lines.into_iter().skip(scroll).collect();
        f.render_widget(Paragraph::new(body), column);
    }
}

/// One group's heading and rows, with the keys in a column of their own.
/// Aligning them is what lets the eye run down the keys looking for one,
/// which is how this window is actually read.
fn block_of(title: &str, listed: Vec<(&str, &str)>, th: Theme) -> Vec<Line<'static>> {
    let gutter = listed
        .iter()
        .map(|(key, _)| key.chars().count())
        .max()
        .unwrap_or(0);
    let mut lines = vec![Line::from(Span::styled(
        title.to_string(),
        Style::default().fg(th.text).add_modifier(Modifier::BOLD),
    ))];
    for (key, what) in listed {
        lines.push(Line::from(vec![
            Span::styled(format!("{key:>gutter$}  "), Style::default().fg(th.accent)),
            Span::styled(what.to_string(), Style::default().fg(th.muted)),
        ]));
    }
    lines.push(Line::raw(""));
    lines
}

/// Holds the scroll inside the content, so a keymap shorter than the last
/// one cannot leave the window scrolled past its own end.
fn clamp_scroll(app: &mut App, lines: usize, height: usize) -> usize {
    let max = lines.saturating_sub(height);
    let Some(help) = &mut app.help else {
        return 0;
    };
    help.scroll = help.scroll.min(max);
    help.scroll
}
