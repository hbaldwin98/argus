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
/// The left half counts the fleet, on loan to whatever the last action
/// reported. `App::on_key` hands it back on the next keypress, so a report
/// is read once and then gets out of the way.
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
            PickerKind::NoteRecipient { .. } => {
                &["j/k move   enter forward   esc cancel", "enter forward  esc"]
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
                    "j/k move  space tick  f line  F note  i insert  o new line  q close",
                    "j/k  space tick  f line  F note  i insert  q close",
                    "f line  F note  i insert  q close",
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
    } else if app.view != View::Spine {
        // A view owns the whole content area and has its own keys. Without
        // this the bar falls through to the spine's columns and advertises
        // keys that do nothing here, which is worse than saying nothing.
        match app.view {
            View::Decisions => (
                &[
                    "h/l features/tree  j/k move  e brief  d/u ten  g/G ends  r refresh  q spine",
                    "h/l features/tree  j/k move  e brief  r refresh  q spine",
                    "h/l  j/k  e brief  q spine",
                ][..],
                th.dim,
            ),
            View::Board => (
                &[
                    "h/l column  j/k card  H/L move it  s send back  e brief  enter tasks  D decisions  q spine",
                    "h/l column  j/k card  H/L move  e brief  enter tasks  q",
                    "h/l  j/k  H/L move  enter tasks  q",
                ][..],
                th.dim,
            ),
            View::Tasks if app.task_input.is_some() => (
                &[
                    "typing — enter saves it, esc throws it away",
                    "enter saves  esc drops",
                ][..],
                th.accent,
            ),
            View::Tasks => (
                &[
                    "h/l column  j/k card  a add  e rewrite  H/L move it  J/K order  x drop  q spine",
                    "h/l  j/k  a add  e rewrite  H/L move  J/K order  x drop  q",
                    "j/k  a add  e rewrite  H/L move  q",
                ][..],
                th.dim,
            ),
            View::Spine => unreachable!("the spine is not a view with its own keys"),
        }
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
    let left = if !app.status.is_empty() {
        vec![Span::styled(
            app.status.clone(),
            Style::default().fg(if alert { th.err } else { th.text }),
        )]
    } else {
        let fleet = fleet(app, th);
        if fleet.is_empty() {
            vec![Span::styled(breadcrumb(app), Style::default().fg(th.muted))]
        } else {
            fleet
        }
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

    let left_len: usize = left.iter().map(Span::width).sum();
    let width = area.width as usize;
    let len = |hint: &String| hint.chars().count();
    let hints = &tiers;
    let beside = hints.iter().find(|h| left_len + len(h) + 3 <= width);
    let alone = || hints.iter().find(|h| len(h) + 2 <= width);

    let mut spans = vec![Span::raw(" ")];
    match (beside, alert) {
        (Some(hint), _) => {
            spans.extend(left);
            spans.push(Span::raw(" ".repeat(width - left_len - len(hint) - 2)));
            spans.push(Span::styled(hint.clone(), Style::default().fg(tone)));
        }
        // Not enough room for both: the alert stays, the keymap goes. The
        // keys are discoverable elsewhere; a swallowed error is not.
        (None, true) => spans.extend(left),
        (None, false) => {
            if let Some(hint) = alone() {
                spans.push(Span::raw(" ".repeat(width.saturating_sub(len(hint) + 2))));
                spans.push(Span::styled(hint.clone(), Style::default().fg(tone)));
            }
        }
    }

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// What the whole fleet is doing, in the left of the bar.
///
/// That seat used to hold a breadcrumb, and a breadcrumb there says nothing
/// new: in the columns it repeats the word written on the card above it,
/// and everywhere else it repeats the path already spelled out across the
/// live view's title. What is *not* written anywhere on screen is the state
/// of the agents you are not currently looking at — which is the entire
/// reason this program has a pane list at all.
///
/// Ordered by urgency, so the count you have to do something about is the
/// one nearest the corner your eye already goes to. Empty when nothing is
/// happening, and the breadcrumb comes back: a bar reading `0 working` is a
/// row spent saying no.
fn fleet(app: &App, th: Theme) -> Vec<Span<'static>> {
    let mut tally: Vec<(PaneStatus, usize)> = Vec::new();
    let states = app
        .tree
        .iter()
        .flat_map(|p| p.repositories.iter())
        .flat_map(|r| r.checkouts.iter())
        .flat_map(|c| c.listed_panes())
        // Children count as their own agents here, the same way they do in
        // the panes column: one of them waiting is a person being waited on.
        .flat_map(|p| std::iter::once(p.status).chain(p.children.iter().map(|c| c.status)));
    for status in states {
        // Idle and exited are not news. Counting them gives the bar a
        // number that is the same whether anything is happening or not.
        if !matches!(
            status,
            PaneStatus::Waiting
                | PaneStatus::Failed
                | PaneStatus::NeedsReview
                | PaneStatus::Working
                | PaneStatus::Done
        ) {
            continue;
        }
        match tally.iter_mut().find(|(s, _)| *s == status) {
            Some((_, n)) => *n += 1,
            None => tally.push((status, 1)),
        }
    }
    tally.sort_by_key(|(s, _)| std::cmp::Reverse(s.urgency()));

    let mut spans = Vec::new();
    for (status, n) in tally {
        if !spans.is_empty() {
            spans.push(Span::raw("   "));
        }
        // The same glyph the rows use, so the count and the column it is
        // counting are read as the same thing.
        spans.push(status_dot(Some(status), th));
        spans.push(Span::styled(
            format!("{n} {}", tally_word(status)),
            Style::default().fg(if status.needs_you() { th.err } else { th.muted }),
        ));
    }
    spans
}

/// Phrased for a count rather than for a row: "2 need you", not "2 needs
/// you", and short enough that three of them still leave the keymap room.
fn tally_word(status: PaneStatus) -> &'static str {
    match status {
        PaneStatus::Waiting => "need you",
        PaneStatus::Failed => "failed",
        PaneStatus::NeedsReview => "to review",
        PaneStatus::Working => "working",
        _ => "done",
    }
}

pub(super) fn breadcrumb(app: &App) -> String {
    match app.focus {
        Focus::Projects => "projects".to_string(),
        _ => content_title(app),
    }
}
