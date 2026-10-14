//! The bottom bar: where you are, what just happened, and the keys that
//! apply here.

use super::*;

use crate::app::Lens;

/// The status bar: where you are on the left, what you can press on the
/// right. Context-sensitive, because the same key means different things
/// inside a pane and in the nav columns.
///
/// Every bar is given as tiers, longest first, and the widest one that
/// fits is what gets drawn. Most modes' tiers are read off the keymap they
/// declare; the ones written out below depend on more than the mode. A single string would be cut mid-word on a
/// narrow terminal -- "j/k move  l open  b branch  B all  F fe" -- which
/// spends the same row on strictly less. Which keys to drop is a judgement
/// about what is worth knowing, so it is made here rather than left to
/// whichever character the width happens to land on.
///
/// The left half counts the fleet, on loan to whatever the last action
/// reported. `App::on_key` hands it back on the next keypress, so a report
/// is read once and then gets out of the way.
pub(super) fn render_status(f: &mut Frame, app: &App, area: Rect, th: Theme) {
    let area = {
        let block = Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(th.edge));
        let inner = block.inner(area);
        f.render_widget(block, area);
        let rail = app.layout.rail.outer;
        if rail.width > 0 {
            f.render_widget(
                Paragraph::new(Span::styled("┴", Style::default().fg(th.edge))),
                Rect {
                    x: rail.right().saturating_sub(1),
                    y: area.y,
                    width: 1,
                    height: 1,
                },
            );
        }
        inner
    };

    let hints: &[&str] = if app.help.is_some() {
        // The keymap window is up, so the bar stops advertising keys and
        // says how to work the window instead.
        &["j/k scroll   any other key closes", "any key closes"][..]
    } else {
        match app.mode() {
            Mode::Prompt => &[
                "type to edit   enter confirm   esc cancel",
                "enter confirm  esc",
            ][..],
            Mode::DirPicker => &[
                "type to filter   ↑/↓ move   → into   ← up   enter choose   esc cancel",
                "↑/↓ move  → into  ← up  enter choose  esc",
                "enter choose  esc",
            ][..],
            Mode::Picker => {
                let Some(p) = &app.picker else {
                    return;
                };
                // What Enter does differs per picker, and "spawn" on the theme list
                // would be a small lie.
                match p.kind {
                    PickerKind::Agent => &["j/k move   enter spawn   esc cancel", "enter spawn  esc"],
                    PickerKind::Workspace { .. } => &[
                        "type to filter or name a new one   ↑/↓ move   enter open   esc cancel",
                        "type to filter   ↑/↓ move   enter open   esc cancel",
                        "enter open  esc",
                    ],
                    PickerKind::Host { .. } => &[
                        "type to filter or name a host to connect to   ↑/↓ move   enter open   esc cancel",
                        "type to filter   ↑/↓ move   enter open   esc cancel",
                        "enter open  esc",
                    ],
                    PickerKind::Theme => &["j/k move   enter apply   esc cancel", "enter apply  esc"],
                    PickerKind::Project => &["j/k move   enter open   esc cancel", "enter open  esc"],
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
                    PickerKind::FeatureCheckout { .. } => &[
                        "type to filter   ↑/↓ move   enter transfer   esc cancel",
                        "enter transfer  esc",
                    ],
                    PickerKind::TaskFeature { .. } => &[
                        "type to filter   ↑/↓ move   enter move the task there   esc cancel",
                        "enter move  esc",
                    ],
                    PickerKind::FeatureWait { .. } => &[
                        "type to filter   ↑/↓ move   enter waits on it, or no longer   esc cancel",
                        "enter toggle  esc",
                    ],
                    PickerKind::Ticket { .. } => &[
                        "↑/↓ move   enter open in the browser   esc cancel",
                        "enter open  esc",
                    ],
                }
            }
            Mode::CheckoutFilter => &[
                "type to filter branches   enter apply   esc clear",
                "type to filter   enter apply   esc clear",
                "enter apply  esc clear",
            ][..],
            Mode::Pane | Mode::Overlay(OverlayMode::Pane) if app.leader_pending => {
                if app.pane_fullscreen {
                    &[
                        "leader…   esc back   f restore   N attention   x close",
                        "leader…  esc  f restore  N  x close",
                    ]
                } else {
                    &[
                        "leader…   esc back   f fullscreen   N attention   x close",
                        "leader…  esc  f full  N  x close",
                    ]
                }
            }
            // Read off the keymap each of these declares, which is also what
            // dispatch looks the key up in, so a key cannot reach one and
            // miss the other.
            Mode::Rail
            | Mode::Stage(View::Workspace | View::Checkouts)
            | Mode::Overlay(
                OverlayMode::Review
                | OverlayMode::History
                | OverlayMode::Settings
                | OverlayMode::SequenceDiagram,
            ) => return draw_bar(f, app, area, &app.mode().keymap().tiers(app), th),
            // The two modes have almost no keys in common, so the bar shows
            // the one you are actually in.
            Mode::Overlay(OverlayMode::Brief) => match app.brief.as_ref().map(|v| v.mode) {
                Some(BriefMode::Insert) => &["typing — esc to stop and save", "esc saves"][..],
                _ => return draw_bar(f, app, area, &app.mode().keymap().tiers(app), th),
            },
            Mode::Overlay(OverlayMode::Pane) => &[
                "floating — ctrl-space then esc to close, x to kill   ctrl-v or alt-v paste",
                "floating — ctrl-space then esc, x to kill",
                "ctrl-space esc",
            ][..],
            Mode::Stage(View::Feature) if app.line.is_some() => &[
                "typing — enter saves it, esc throws it away",
                "enter saves  esc drops",
            ][..],
            Mode::Stage(View::Feature) if app.capture.is_some() => &[
                "ctrl-s create   tab indent   shift-tab outdent   esc cancel",
                "ctrl-s create  tab indent  esc cancel",
                "ctrl-s  esc",
            ][..],
            // Written out per level rather than read off the keymap, since
            // which keys are live and what they are called both depend on
            // where the stage is: `a` is a feature on the index, a task in
            // one, and a flow on the flows lens.
            Mode::Stage(View::Feature) if !app.feature_open => &[
                "j/k move  enter open  a new  e brief  R rename  m checkout  p hold  w after  t ticket  T link  M map  v archive  z folded  x drop  g pane  r refresh  q workspace",
                "j/k move  enter open  a new  m checkout  p hold  w after  t ticket  v archive  z folded  q workspace",
                "j/k  enter open  a new  v archive  q",
            ][..],
            Mode::Stage(View::Feature) if app.in_detail => &[
                "j/k move  space check or take it  x drop  tab list  esc up",
                "j/k  space  tab list  esc",
            ][..],
            Mode::Stage(View::Feature) => match app.lens {
                Lens::Work => &[
                    "j/k move  h/l fold  space done  a task  s subtask  d decision  c criterion  n note  i note  enter brief  H/L state  J/K order  >/< nest  m feature  t ticket  T link  x drop  tab detail  2 record  esc up",
                    "j/k move  h/l fold  space done  a task  s subtask  d decision  t ticket  tab detail  2 record  esc up",
                    "j/k  space done  a task  s subtask  tab  esc",
                ][..],
                Lens::Record => &[
                    "j/k move  d new decision  s sub-decision  u supersede  e edit  y copy  o answer  x withdraw  t ticket  tab detail  1 work  3 flows  esc up",
                    "j/k move  d new decision  s sub-decision  u supersede  tab detail  1 work  esc up",
                    "j/k  d decision  tab  1 work  esc",
                ][..],
                Lens::Flows => &[
                    "j/k step  h/l flow  d bind a decision  e edit  a new flow  enter open  x drop  t ticket  1 work  2 record  esc up",
                    "j/k step  h/l flow  d bind  e edit  enter open  1 work  2 record  esc up",
                    "j/k  h/l flow  enter  esc",
                ][..],
            },
            // A parked pane is not taking input anywhere the operator can
            // see, so the way back to the live screen outranks the usual
            // keymap.
            Mode::Pane if app.scroll_indicator().is_some() => &[
                "scrolled back   shift-pgup/pgdn move   type or scroll down to return",
                "scrolled back   type or scroll down to return",
                "scrolled back — type to return",
            ][..],
            Mode::Pane if app.pane_fullscreen => &[
                "typing   ctrl-space: esc leave  f restore  x close   shift-pgup scroll",
                "typing   ctrl-space: esc leave  f restore  x close",
                "typing   ctrl-space esc",
            ][..],
            Mode::Pane => &[
                "typing   ctrl-space: esc leave  f fullscreen  x close   shift-pgup scroll",
                "typing   ctrl-space: esc leave  f full  x close",
                "typing   ctrl-space esc",
            ][..],
        }
    };

    draw_bar(f, app, area, hints, th);
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
    } else if let Some(scope) = app.checkout_filter_scope() {
        // The filter query lives only in app state until drawn; the bar is
        // where the operator looks while typing.
        vec![Span::styled(
            scope,
            Style::default().fg(if app.checkout_filtering {
                th.accent
            } else {
                th.muted
            }),
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

    let (pad, tone) = (2, th.dim);
    let width = width.saturating_sub(pad - 1);
    let mut spans = vec![Span::raw(" ".repeat(pad))];
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
/// new: it repeats the path already spelled out across the workspace
/// heading. What is *not* written anywhere on screen is the state of the
/// agents you are not currently looking at — the rail shows one project,
/// and one repository of it open — which is the entire reason the bar
/// counts them.
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
        .flat_map(|c| c.statuses());
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
        // The shell's palette: one color per state, glyph and count alike.
        let color = status_color(status, th);
        let glyph = match status {
            PaneStatus::Working => crate::motion::spinner(app.frame_now(), app.epoch()),
            PaneStatus::Done => "✓",
            _ => "▲",
        };
        spans.push(Span::styled(
            format!("{glyph} {n} {}", tally_word(status)),
            Style::default().fg(color),
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
