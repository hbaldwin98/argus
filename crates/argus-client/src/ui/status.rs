//! The bottom bar: where you are, what just happened, and the keys that
//! apply here.

use super::*;

/// The status bar: where you are on the left, what you can press on the
/// right. Context-sensitive, because the same key means different things
/// inside a pane and in the nav columns.
///
/// Every keymap is given as tiers, longest first, and the widest one that
/// fits is what gets drawn. A single string would be cut mid-word on a
/// narrow terminal -- "j/k move  l open  b branch  B all  F fe" -- which
/// spends the same row on strictly less. Which keys to drop is a judgement
/// about what is worth knowing, so it is made here rather than left to
/// whichever character the width happens to land on.
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

    let (hints, tone) = if app.help.is_some() {
        // The keymap window is up, so the bar stops advertising keys and
        // says how to work the window instead.
        (
            &["j/k scroll   any other key closes", "any key closes"][..],
            th.dim,
        )
    } else if let Some(p) = &app.picker {
        // What Enter does differs per picker, and "spawn" on the theme list
        // would be a small lie.
        let hints: &[&str] = match p.kind {
            PickerKind::Agent => &["j/k move   enter spawn   esc cancel", "enter spawn  esc"],
            PickerKind::Workspace { .. } => &[
                "type to filter or name a new one   ↑/↓ move   enter open   esc cancel",
                "type to filter   ↑/↓ move   enter open   esc cancel",
                "enter open  esc",
            ],
            PickerKind::Theme => &["j/k move   enter apply   esc cancel", "enter apply  esc"],
            PickerKind::Branch { .. } => &[
                "type to filter   ↑/↓ move   enter switch   esc cancel",
                "enter switch  esc",
            ],
            PickerKind::File { .. } => &[
                "type to filter   ↑/↓ move   enter open   esc cancel",
                "enter open  esc",
            ],
            PickerKind::Change => &[
                "type to filter   ↑/↓ move   enter jump   esc cancel",
                "enter jump  esc",
            ],
            PickerKind::ReviewRecipient { .. } => {
                &["j/k move   enter send   esc cancel", "enter send  esc"]
            }
        };
        (hints, th.dim)
    } else if app.prompt.is_some() {
        (
            &["type to edit   enter confirm   esc cancel", "enter confirm  esc"][..],
            th.dim,
        )
    } else if app.leader_pending {
        let hints: &[&str] = if app.pane_fullscreen {
            &[
                "leader…   esc back   f restore   N attention   x close",
                "leader…  esc  f restore  N  x close",
            ]
        } else {
            &[
                "leader…   esc back   f fullscreen   N attention   x close",
                "leader…  esc  f full  N  x close",
            ]
        };
        (hints, th.accent)
    } else if matches!(app.overlay, Some(Overlay::Settings { .. })) {
        (
            &["j/k move   h/l change   esc close", "h/l change  esc"][..],
            th.dim,
        )
    } else if matches!(app.overlay, Some(Overlay::Review)) {
        // A commit reached from the history overlay goes back to it rather
        // than flipping a side that means nothing there. `s` names where it
        // would take you, not where you are.
        let from_history = app
            .review
            .as_ref()
            .is_some_and(|v| v.review.commit.is_some())
            && app.history.is_some();
        let split = if app.review_split { "s unified" } else { "s split" };
        let base = if from_history {
            "h history"
        } else {
            "b staged/unstaged"
        };
        // Built rather than matched out: the two switches are independent,
        // and four spelled-out combinations times three tiers is twelve
        // strings nobody could keep in step.
        let hints: [String; 3] = [
            format!("j/k  ]/[ file  f jump  c comment  e edit  {split}  {base}  esc close"),
            format!("j/k  ]/[ file  c comment  {split}  {base}  esc"),
            format!("]/[ file  c comment  {split}  esc"),
        ];
        return draw_bar(f, app, area, &hints, th.dim, th);
    } else if matches!(app.overlay, Some(Overlay::History)) {
        (
            &[
                "j/k  ]/[ commit  l files/open  h fold  r refresh  R review  esc close",
                "]/[ commit  l open  h fold  R review  esc",
                "]/[ commit  R review  esc",
            ][..],
            th.dim,
        )
    } else if matches!(app.overlay, Some(Overlay::Notes)) {
        // The two modes have almost no keys in common, so the bar shows
        // the one you are actually in.
        match app.notes.as_ref().map(|v| v.mode) {
            Some(NoteMode::Insert) => (
                &["typing — esc to stop and save", "esc saves"][..],
                th.accent,
            ),
            _ => (
                &[
                    "j/k move  0/$ line  space tick  i insert  o new line  q close",
                    "j/k  space tick  i insert  o new line  q close",
                    "space tick  i insert  q close",
                ][..],
                th.dim,
            ),
        }
    } else if app.overlay.is_some() {
        (
            &[
                "floating — ctrl-space then esc to close, x to kill   ctrl-v paste",
                "floating — ctrl-space then esc, x to kill",
                "ctrl-space esc",
            ][..],
            th.dim,
        )
    } else if app.focus == Focus::PaneContent {
        // A parked pane is not taking input anywhere the operator can see,
        // so the way back to the live screen outranks the usual keymap.
        if app.scroll_indicator().is_some() {
            (
                &[
                    "scrolled back   shift-pgup/pgdn move   type or scroll down to return",
                    "scrolled back   type or scroll down to return",
                    "scrolled back — type to return",
                ][..],
                th.accent,
            )
        } else if app.pane_fullscreen {
            (
                &[
                    "typing   ctrl-space: esc leave  f restore  x close   shift-pgup scroll",
                    "typing   ctrl-space: esc leave  f restore  x close",
                    "typing   ctrl-space esc",
                ][..],
                th.dim,
            )
        } else {
            (
                &[
                    "typing   ctrl-space: esc leave  f fullscreen  x close   shift-pgup scroll",
                    "typing   ctrl-space: esc leave  f full  x close",
                    "typing   ctrl-space esc",
                ][..],
                th.dim,
            )
        }
    } else {
        // Per column rather than one list of everything: the bar cannot
        // hold every key at once, and most of them only apply somewhere.
        let keys: &[&str] = match app.focus {
            Focus::Projects => &[
                "j/k  l open  n add  D rm  w wksp  p fold",
                "l open  n add  p fold",
                "l open  n add",
            ],
            Focus::Repositories => &[
                "j/k  l open  s shell  a agent  b branch  n add",
                "l open  s shell  a agent",
                "l open  a agent",
            ],
            Focus::Checkouts => &[
                "j/k  l open  b branch  F fetch  R review  H history",
                "l open  R review  H history",
                "l open  R review",
            ],
            _ => &[
                "j/k  l open  s shell  a agent  R review  x close",
                "l open  a agent  R review  x close",
                "l open  R review",
            ],
        };
        (keys, th.dim)
    };

    draw_bar(f, app, area, hints, tone, th);
}

/// Lays the chosen tiers out against the space there is.
///
/// The keymap is what the user acts on, so it wins: the widest tier that
/// still leaves the breadcrumb a real gap is preferred, and failing that
/// the breadcrumb is dropped and the widest tier that fits on its own is
/// drawn. Only when none of them fits does the bar give up on the keys.
const ASK: &str = "? keys";

fn draw_bar<S: AsRef<str>>(
    f: &mut Frame,
    app: &App,
    area: Rect,
    hints: &[S],
    tone: Color,
    th: Theme,
) {
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

    // Every context ends at the same place: the one key that lists the
    // rest. It is appended rather than written into each tier so it cannot
    // be the thing a narrow bar drops, and it earns its cell by letting
    // every tier above it be shorter than it used to be.
    let mut tiers: Vec<String> = Vec::with_capacity(hints.len() + 1);
    if app.help.is_some() {
        tiers.extend(hints.iter().map(|h| h.as_ref().to_string()));
    } else {
        tiers.extend(hints.iter().map(|h| format!("{}   {ASK}", h.as_ref())));
        tiers.push(ASK.to_string());
    }

    let left_len = left.content.chars().count();
    let width = area.width as usize;
    let len = |hint: &String| hint.chars().count();
    let hints = &tiers;
    let beside = hints.iter().find(|h| left_len + len(h) + 3 <= width);
    let alone = || hints.iter().find(|h| len(h) + 2 <= width);

    let mut spans = vec![Span::raw(" ")];
    match (beside, alert) {
        (Some(hint), _) => {
            spans.push(left);
            spans.push(Span::raw(" ".repeat(width - left_len - len(hint) - 2)));
            spans.push(Span::styled(hint.clone(), Style::default().fg(tone)));
        }
        // Not enough room for both: the alert stays, the keymap goes. The
        // keys are discoverable elsewhere; a swallowed error is not.
        (None, true) => spans.push(left),
        (None, false) => {
            if let Some(hint) = alone() {
                spans.push(Span::raw(" ".repeat(width.saturating_sub(len(hint) + 2))));
                spans.push(Span::styled(hint.clone(), Style::default().fg(tone)));
            }
        }
    }

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

pub(super) fn breadcrumb(app: &App) -> String {
    match app.focus {
        Focus::Projects => "projects".to_string(),
        _ => content_title(app),
    }
}
