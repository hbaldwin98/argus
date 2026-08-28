//! The bottom bar: where you are, what just happened, and the keys that
//! apply here.

use super::*;

/// The status bar: where you are on the left, what you can press on the
/// right. Context-sensitive, because the same key means different things
/// inside a pane and in the nav columns.
///
/// The left half is the breadcrumb's seat, on loan to whatever the last
/// action reported. `App::on_key` hands it back on the next keypress, so a
/// report is read once and then gets out of the way.
pub(super) fn render_status(f: &mut Frame, app: &App, area: Rect, th: Theme) {
    // `area` includes the blank padding row; the bar is its last row.
    let area = Rect {
        y: area.y + area.height.saturating_sub(1),
        height: area.height.min(1),
        ..area
    };

    let (hint, tone) = if let Some(p) = &app.picker {
        // What Enter does differs per picker, and "spawn" on the theme list
        // would be a small lie.
        let hint = match p.kind {
            PickerKind::Agent => "j/k move   enter spawn   esc cancel",
            PickerKind::Workspace { .. } => {
                "type to filter or name a new one   ↑/↓ move   enter open   esc cancel"
            }
            PickerKind::Theme => "j/k move   enter apply   esc cancel",
            PickerKind::Branch { .. } => "type to filter   ↑/↓ move   enter switch   esc cancel",
            PickerKind::File { .. } => "type to filter   ↑/↓ move   enter open   esc cancel",
            PickerKind::Change => "type to filter   ↑/↓ move   enter jump   esc cancel",
            PickerKind::ReviewRecipient { .. } => "j/k move   enter send   esc cancel",
        };
        (hint, th.dim)
    } else if app.prompt.is_some() {
        ("type to edit   enter confirm   esc cancel", th.dim)
    } else if app.leader_pending {
        let hint = if app.pane_fullscreen {
            "leader…   esc back   f restore   N attention   x close"
        } else {
            "leader…   esc back   f fullscreen   N attention   x close"
        };
        (hint, th.accent)
    } else if matches!(app.overlay, Some(Overlay::Settings { .. })) {
        ("j/k move   h/l change   esc close", th.dim)
    } else if matches!(app.overlay, Some(Overlay::Review)) {
        // A commit reached from the history overlay goes back to it rather
        // than flipping a side that means nothing there. `s` names where it
        // would take you, not where you are.
        let from_history = app
            .review
            .as_ref()
            .is_some_and(|v| v.review.commit.is_some())
            && app.history.is_some();
        let hint = match (from_history, app.review_split) {
            (true, false) => {
                "j/k  ]/[ file  f jump  c comment  e edit  s split  h history  esc close"
            }
            (true, true) => {
                "j/k  ]/[ file  f jump  c comment  e edit  s unified  h history  esc close"
            }
            (false, false) => {
                "j/k  ]/[ file  f jump  c comment  e edit  s split  b staged/unstaged  esc close"
            }
            (false, true) => {
                "j/k  ]/[ file  f jump  c comment  e edit  s unified  b staged/unstaged  esc close"
            }
        };
        (hint, th.dim)
    } else if matches!(app.overlay, Some(Overlay::History)) {
        (
            "j/k  ]/[ commit  l files/open  h fold  r refresh  R review  esc close",
            th.dim,
        )
    } else if matches!(app.overlay, Some(Overlay::Notes)) {
        // The two modes have almost no keys in common, so the bar shows
        // the one you are actually in.
        match app.notes.as_ref().map(|v| v.mode) {
            Some(NoteMode::Insert) => ("typing — esc to stop and save", th.accent),
            _ => (
                "j/k move  0/$ line  space tick  i insert  o new line  q close",
                th.dim,
            ),
        }
    } else if app.overlay.is_some() {
        (
            "floating — ctrl-space then esc to close, x to kill   ctrl-v paste",
            th.dim,
        )
    } else if app.focus == Focus::PaneContent {
        // A parked pane is not taking input anywhere the operator can see,
        // so the way back to the live screen outranks the usual keymap.
        if app.scroll_indicator().is_some() {
            (
                "scrolled back   shift-pgup/pgdn move   type or scroll down to return",
                th.accent,
            )
        } else if app.pane_fullscreen {
            (
                "typing   ctrl-space: esc leave  f restore  x close   shift-pgup scroll",
                th.dim,
            )
        } else {
            (
                "typing   ctrl-space: esc leave  f fullscreen  x close   shift-pgup scroll",
                th.dim,
            )
        }
    } else {
        // Per column rather than one list of everything: the bar cannot
        // hold every key at once, and most of them only apply somewhere.
        let keys = match app.focus {
            Focus::Projects => {
                "j/k  l open  N needs  n add  D rm  w wksp  p fold  S settings  q detach"
            }
            Focus::Repositories => {
                "j/k move  l open  N attention  s shell  a agent  b branch  f file  n add  i init  D rm  q detach"
            }
            Focus::Checkouts => {
                "j/k move  l open  b branch  B all  F fetch  P pull  f file  R review  H history  n worktree  D rm  q detach"
            }
            _ => "j/k move  l open  v panes  N attention  s shell  a agent  b branch  f file  R review  H history  x close  q detach",
        };
        (keys, th.dim)
    };

    // An alert is the one thing on this bar the user *must* read, so it
    // outranks the keymap for space. An ordinary report is news rather than
    // an alarm: brighter than the breadcrumb it stands in for, but it yields
    // to the keys the same way the breadcrumb does.
    let alert = app.status_alert;
    let left = if app.status.is_empty() {
        Span::styled(breadcrumb(app), Style::default().fg(th.muted))
    } else {
        Span::styled(
            app.status.clone(),
            Style::default().fg(if alert { th.err } else { th.text }),
        )
    };

    let hint_span = Span::styled(hint, Style::default().fg(tone));

    // The keymap is what the user acts on, so it wins the space. If the
    // breadcrumb can't fit beside it with a real gap, drop the breadcrumb
    // rather than letting the two run together and truncate the keys.
    let hint_len = hint_span.content.chars().count();
    let left_len = left.content.chars().count();
    let width = area.width as usize;

    let mut spans = vec![Span::raw(" ")];
    if left_len + hint_len + 3 <= width {
        spans.push(left);
        spans.push(Span::raw(" ".repeat(width - left_len - hint_len - 2)));
        spans.push(hint_span);
    } else if alert {
        // Not enough room for both: the alert stays, the keymap goes. The
        // keys are discoverable elsewhere; a swallowed error is not.
        spans.push(left);
    } else {
        spans.push(Span::raw(" ".repeat(width.saturating_sub(hint_len + 2))));
        spans.push(hint_span);
    }

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

pub(super) fn breadcrumb(app: &App) -> String {
    match app.focus {
        Focus::Projects => "projects".to_string(),
        _ => content_title(app),
    }
}
