//! A card and the rows in it: its frame and title, one row's name, detail
//! and badge, and how a list longer than the card is scrolled and admits
//! it. Every list surface draws through here — the pickers, the modals and
//! the stages — so a row and a scrollbar look the same wherever they are.

use super::*;

/// The mark that says a card holds more than it is showing.
///
/// A card scrolls silently: the rows slide under the cursor and nothing on
/// screen ever admits there were more of them, so a list of twenty
/// checkouts in a card that fits six looks exactly like a list of six. The
/// thumb goes in the blank padding cell between the rows and the border,
/// which costs the names nothing.
pub(super) fn render_overflow(
    f: &mut Frame,
    inner: Rect,
    first: usize,
    visible: usize,
    len: usize,
    lit: crate::motion::Lit,
    th: Theme,
) {
    if inner.height == 0 || visible == 0 || len <= visible {
        return;
    }
    let track = inner.height as usize;
    // Proportional, but never smaller than a cell: a thumb that rounds to
    // nothing is a scrollbar that disappears exactly when the list is long
    // enough to need one.
    let thumb = (track * visible / len).clamp(1, track);
    let span = track - thumb;
    let start = span * first / (len - visible);
    let style = Style::default().fg(crate::motion::blend(th.dim, th.accent, lit.value()));
    for i in 0..thumb {
        let y = inner.y + (start + i) as u16;
        f.render_widget(
            Paragraph::new(Span::styled(SCROLL_THUMB, style)),
            Rect::new(inner.right(), y, 1, 1),
        );
    }
}

/// Where a card's window sits after this frame: `scrolled_to` is where
/// the last frame left it, and it moves as little as it can to keep the
/// selection on screen.
///
/// Deliberately not derived from the selection alone. Doing that pins the
/// selected row to the bottom of the card the moment the list is longer
/// than the card — nothing below the cursor is ever visible, every step
/// drags the whole list under it, and a row appearing above the selection
/// (a branch losing its checkout pins one) makes the column lurch.
pub(super) fn scrolled_to_show(
    scrolled_to: usize,
    selected: Option<usize>,
    visible: usize,
    len: usize,
) -> usize {
    if visible == 0 || len <= visible {
        return 0;
    }
    // Never leave blank rows below a list that could fill them.
    let mut first = scrolled_to.min(len - visible);
    if let Some(selected) = selected {
        if selected < first {
            first = selected;
        } else if selected >= first + visible {
            first = selected + 1 - visible;
        }
    }
    first
}

/// An item: name, then — when the card is tall enough to afford it — a
/// detail line. The selection is a raised bar over the whole row with an
/// accent marker pinning the first line; unselected rows get a blank
/// gutter so text lines up either way. `dim` spans would sink into the
/// selection fill, so they are lifted to `muted` there.
pub(super) fn render_row<'a>(
    f: &mut Frame,
    area: Rect,
    item: Item<'a>,
    selected: bool,
    lit: impl Into<crate::motion::Lit>,
    th: Theme,
) {
    let lit = lit.into();
    let focused = lit.is_lit();
    // The selection bar travels with the card it is in: the row you are on
    // brightens as focus arrives and dims as it leaves, so the selection
    // and the border are one movement rather than two.
    let bar = match selected {
        true => Style::default().bg(crate::motion::blend(th.sel_bg_dim, th.sel_bg, lit.value())),
        false => Style::default(),
    };

    let marker = if selected && focused {
        Span::styled(MARKER, Style::default().fg(th.accent).patch(bar))
    } else {
        Span::styled(GUTTER, bar)
    };
    // A card nobody is in is background: every card shouting in the same
    // weight reads as several of the same thing rather than as one place
    // with the keys in it. The selected row keeps its weight even so.
    let recede = !focused && !selected;
    let lift = |spans: Vec<Span<'a>>| -> Vec<Span<'a>> {
        spans
            .into_iter()
            .map(|s| {
                let mut style = s.style.patch(bar);
                if selected && style.fg == Some(th.dim) {
                    style = style.fg(th.muted);
                }
                if recede && style.fg == Some(th.text) {
                    style = style.fg(th.muted).remove_modifier(Modifier::BOLD);
                }
                Span::styled(s.content, style)
            })
            .collect()
    };

    // The deeper indent buys alignment, and it is only worth having while
    // it is free: a detail line that would be ellipsized to pay for a tidy
    // left edge has traded something the user reads for something they
    // merely notice. "no checkout" beats "no checko…".
    let detail_width: usize = item.detail.iter().map(Span::width).sum();
    let indent = if 1 + item.indent + detail_width <= area.width as usize {
        item.indent
    } else {
        STATUS_WIDTH
    };
    let mut name = vec![marker];
    name.extend(lift(item.name));

    let width = area.width as usize;
    let badge = lift(item.badge);
    let name = if badge.is_empty() {
        ellipsize_spans(name, width)
    } else {
        // The badge is a count, and a count survives truncation better than
        // the tail of a name does: "argus-cl…" with a `4 ▣` beside it says
        // more than the two extra letters would. So the badge is reserved
        // for first and the name ellipsized around it — unless doing that
        // would leave the name too short to identify anything, in which
        // case the badge is the part that goes.
        let badge_width: usize = badge.iter().map(Span::width).sum();
        // A cell is kept past the badge so it ends inside the row rather
        // than against its edge. The selection fill runs the whole width,
        // and a count flush to the end of it reads as having escaped the
        // highlight — the left edge has a marker and a space, so the right
        // needs the space to answer it.
        match width
            .checked_sub(badge_width + 2)
            .filter(|room| *room >= NAME_FLOOR)
        {
            Some(room) => {
                let mut name = ellipsize_spans(name, room);
                let used: usize = name.iter().map(Span::width).sum();
                name.push(Span::styled(
                    " ".repeat(width - used - badge_width - 1),
                    bar,
                ));
                name.extend(badge);
                name.push(Span::styled(GUTTER, bar));
                name
            }
            None => ellipsize_spans(name, width),
        }
    };

    let mut lines = vec![Line::from(name)];
    if area.height >= ROW_HEIGHT {
        let mut detail = vec![
            Span::styled(GUTTER, bar),
            Span::styled(" ".repeat(indent), bar),
        ];
        detail.extend(lift(item.detail));
        lines.push(Line::from(ellipsize_spans(detail, width)));
    }

    f.render_widget(Paragraph::new(lines).style(bar), area);
}

/// A padded card. Focus has to be unmissable at a glance, so the focused
/// panel is lifted a step in elevation and given an accent border and
/// title, against the unfocused panels' receding edge and muted label.
/// `lit` is how focused the card is, which is a number rather than a flag
/// because focus fades from one card to the next: the border, the title
/// and the fill all travel between their two ends together. Call sites
/// that are simply focused or not still pass `true` or `false`.
pub(super) fn panel_block(
    title: &str,
    lit: impl Into<crate::motion::Lit>,
    th: Theme,
    width: u16,
) -> Block<'_> {
    let lit = lit.into();
    let t = lit.value();
    let border = Style::default().fg(crate::motion::blend(th.edge, th.accent, t));
    // Weight cannot be blended, so it flips at the midpoint. Over 120ms
    // that reads as part of the same movement rather than as a second one.
    let label = match lit.is_lit() {
        true => Style::default()
            .fg(crate::motion::blend(th.muted, th.accent, t))
            .add_modifier(Modifier::BOLD),
        false => Style::default().fg(crate::motion::blend(th.muted, th.accent, t)),
    };
    let fill = crate::motion::blend(th.surface, th.surface_focus, t);
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border)
        .style(Style::default().bg(fill))
        // The inner gutter is what stops text from sitting on the border.
        .padding(Padding::new(1, 1, 1, 0))
        .title(Span::styled(
            format!(
                " {} ",
                ellipsize_text(title, width.saturating_sub(4) as usize)
            ),
            label,
        ))
}

pub(super) fn row_rect_of(inner: Rect, i: usize, height: u16) -> Option<Rect> {
    let offset = u16::try_from(i).ok()?.checked_mul(height)?;
    let y = inner.y.checked_add(offset)?;
    (y + height <= inner.y + inner.height).then(|| Rect::new(inner.x, y, inner.width, height))
}
