//! The keymap, as a window you ask for rather than a bar you read.
//!
//! The status bar holds about a dozen keys on a wide terminal and half that
//! on a narrow one, so it answered "what can I press here?" by not
//! answering — it showed the keys that fit and left the rest to be
//! discovered. This is the other half: `?` from anywhere that is not a
//! typing surface, which lets the bar go back to being a reminder of the
//! few keys worth having in front of you at all times.

use super::*;

/// One heading and the keys under it. Grouped by what the key acts on
/// rather than alphabetically: every binding sorted by character is a
/// reference, and what somebody pressing `?` wants is an answer.
pub(super) struct Group {
    pub title: &'static str,
    pub keys: &'static [(&'static str, &'static str)],
}

const MOVE: Group = Group {
    title: "moving",
    keys: &[
        ("j / k", "up and down this column"),
        ("l  enter", "into the thing under the cursor"),
        ("h  esc", "back out one column"),
        ("N", "jump to whatever needs attention"),
    ],
};

const SELECTION: Group = Group {
    title: "the thing under the cursor",
    keys: &[
        ("s", "a shell here"),
        ("a", "an agent here"),
        ("b", "switch branch"),
        ("B", "also list branches nothing is on"),
        ("F", "fetch"),
        ("P", "pull"),
        ("f", "open a file"),
        ("R  tab", "review the diff"),
        ("H", "history"),
        ("m", "notes"),
        ("n", "add one"),
        ("i", "init a repository"),
        ("D", "remove it"),
        ("x", "close the pane"),
    ],
};

const VIEW: Group = Group {
    title: "the view",
    keys: &[
        ("1", "the spine — projects through to the live pane"),
        ("2", "the decision board"),
        ("p", "fold a column away, and back"),
        ("v", "where panes are listed"),
        ("t", "theme"),
        ("w", "workspace"),
        ("S", "settings"),
    ],
};

const PANE: Group = Group {
    title: "in a pane",
    keys: &[
        ("", "every key goes to the program"),
        ("ctrl-space esc", "hand the keyboard back"),
        ("ctrl-space f", "fullscreen, and back"),
        ("ctrl-space x", "close it"),
        ("ctrl-space tab", "review"),
        ("ctrl-space H", "history"),
        ("ctrl-space N", "next needing attention"),
        ("ctrl-space 1 / 2", "another view"),
        ("shift-pgup", "back through the scrollback"),
    ],
};

const REVIEW: Group = Group {
    title: "reading a diff",
    keys: &[
        ("j / k", "line by line"),
        ("d / u", "ten at a time"),
        ("] / [", "next and previous file"),
        ("g / G", "top and bottom"),
        ("f", "jump to a change"),
        ("v", "mark a range"),
        ("c", "comment to the agent"),
        ("e", "open it in your editor"),
        ("s", "split and unified"),
        ("b", "staged, unstaged, and the branch"),
        ("H", "history"),
        ("h", "back to the list"),
        ("esc  q", "close"),
    ],
};

const HISTORY: Group = Group {
    title: "reading history",
    keys: &[
        ("j / k", "commit by commit"),
        ("] / [", "next and previous commit"),
        ("l  enter", "its files, then the diff"),
        ("h", "fold it back up"),
        ("g / G", "top and bottom"),
        ("r", "refresh"),
        ("R", "review the working tree instead"),
        ("esc  q", "close"),
    ],
};

const NOTES: Group = Group {
    title: "a note",
    keys: &[
        ("j / k  h / l", "move the cursor"),
        ("0 / $", "start and end of the line"),
        ("space", "tick the box on this line"),
        ("f", "forward this line to an agent"),
        ("F", "forward the whole note to an agent"),
        ("i  a", "start typing"),
        ("o", "a new line below"),
        ("esc", "stop typing, and save"),
        ("q", "close"),
    ],
};

const SETTINGS: Group = Group {
    title: "settings",
    keys: &[
        ("j / k", "move"),
        ("h / l", "change this one"),
        ("esc  q", "close"),
    ],
};

const BOARD: Group = Group {
    title: "the decision board",
    keys: &[
        ("h / l", "the features, and the tree under one"),
        ("j / k", "feature by feature, or decision by decision"),
        ("d / u", "ten at a time"),
        ("g / G", "top and bottom"),
        ("r", "re-ask the daemon for it"),
        ("esc  q", "back to the spine"),
    ],
};

const COLUMNS_BOARD: Group = Group {
    title: "the feature board",
    keys: &[
        ("h / l", "column by column"),
        ("j / k", "card by card"),
        ("d / u", "ten at a time"),
        ("g / G", "top and bottom"),
        ("H / L", "move this card a column"),
        ("s", "send it back to whoever is on it"),
        ("enter", "the tasks under this feature"),
        ("D", "the decisions under it"),
        ("r", "re-ask the daemon for it"),
        ("esc  q", "back to the spine"),
    ],
};

const TASKS: Group = Group {
    title: "a feature's tasks",
    keys: &[
        ("h / l", "column by column"),
        ("j / k", "card by card"),
        ("a", "add one"),
        ("e  enter", "rewrite it"),
        ("H / L", "move this task a column"),
        ("J / K", "earlier or later in the list"),
        ("x", "drop it"),
        ("r", "re-ask the daemon for them"),
        ("esc  q", "back to the spine"),
    ],
};

/// True everywhere, so it is worth saying once rather than per mode.
const EVERYWHERE: Group = Group {
    title: "everywhere",
    keys: &[
        ("?", "this list"),
        ("ctrl-v", "paste"),
        ("F12", "close the floating window, whatever it is"),
        ("q", "detach — the daemon and its panes keep running"),
    ],
};

/// The keys that apply where the user actually is. Asked of the app rather
/// than fixed, because `?` in a diff and `?` in the columns are different
/// questions, and answering both with one wall of text answers neither.
pub(super) fn groups(app: &App) -> Vec<&'static Group> {
    let mut groups = if matches!(app.overlay, Some(Overlay::Review)) || app.focus == Focus::Review {
        vec![&REVIEW]
    } else if matches!(app.overlay, Some(Overlay::History)) {
        vec![&HISTORY]
    } else if matches!(app.overlay, Some(Overlay::Notes)) {
        vec![&NOTES]
    } else if matches!(app.overlay, Some(Overlay::Settings { .. })) {
        vec![&SETTINGS]
    } else if app.focus == Focus::View {
        match app.view {
            crate::app::View::Board => vec![&COLUMNS_BOARD, &VIEW],
            crate::app::View::Tasks => vec![&TASKS, &VIEW],
            _ => vec![&BOARD, &VIEW],
        }
    } else if app.input_pane().is_some() || app.focus == Focus::PaneContent {
        vec![&PANE]
    } else {
        vec![&MOVE, &SELECTION, &VIEW]
    };
    groups.push(&EVERYWHERE);
    groups
}

/// Draws the window over `area`, which is the columns and not the status
/// bar: the bar is where it says how to put the window away.
pub(super) fn render_help(f: &mut Frame, app: &mut App, area: Rect, th: Theme) {
    let blocks: Vec<Vec<Line>> = groups(app).iter().map(|g| block_of(g, th)).collect();
    let content_width = blocks
        .iter()
        .flatten()
        .map(Line::width)
        .max()
        .unwrap_or(0) as u16;
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
    let block = panel_block("keys · esc to close", true, th, popup.width);
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
            Rect { width: half, ..inner },
            Rect { x: inner.x + half, width: inner.width - half, ..inner },
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
fn block_of(group: &Group, th: Theme) -> Vec<Line<'static>> {
    let gutter = group
        .keys
        .iter()
        .map(|(key, _)| key.chars().count())
        .max()
        .unwrap_or(0);
    let mut lines = vec![Line::from(Span::styled(
        group.title.to_string(),
        Style::default().fg(th.text).add_modifier(Modifier::BOLD),
    ))];
    for (key, what) in group.keys {
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
