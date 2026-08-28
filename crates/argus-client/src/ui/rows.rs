//! The vocabulary of a tree row — the glyphs and spans that say what a
//! checkout, branch, pane, or child agent is and what state it is in.
//! Shared by the columns and by the pickers, so a row means the same
//! thing wherever it is drawn.

use super::*;

/// A checkout's row: what is running in it, and what git says about it.
pub(super) fn checkout_item(c: &argus_protocol::CheckoutInfo, th: Theme) -> Item<'static> {
    // A checkout is usually sitting on the branch it's named after;
    // repeating it ("master master") says nothing. Show the branch only
    // when it actually differs.
    let mut detail = git_spans_unless_branch_is(c.git.as_ref(), &c.name, th);
    if detail.is_empty() {
        detail.push(Span::styled(
            if c.primary { "primary" } else { "worktree" },
            Style::default().fg(th.dim),
        ));
    }
    // Two agents in one directory is allowed, but it is never something to
    // find out from the diff later. The glyph carries it where the column
    // is too narrow for the words, which is most columns.
    let agents = c
        .listed_panes()
        .filter(|p| p.kind == argus_protocol::PaneKind::Agent)
        .count();
    let shared = agents > 1;
    if shared {
        detail.push(Span::styled("  ", Style::default()));
        detail.push(Span::styled(
            format!("shared by {agents}"),
            Style::default().fg(th.warn),
        ));
    }
    detail.extend(note_detail(c.notes, c.has_note, th));
    Item::new(
        vec![
            status_dot(worst_pane_status(c), th),
            Span::styled(
                format!("{} ", if c.primary { "⌂" } else { "⧉" }),
                Style::default().fg(if c.primary { th.muted } else { th.dim }),
            ),
            Span::styled(
                c.name.clone(),
                Style::default().fg(th.text).add_modifier(Modifier::BOLD),
            ),
            Span::styled(if shared { " ⚠" } else { "" }, Style::default().fg(th.warn)),
        ],
        detail,
    )
    .badged(if c.listed_panes().next().is_none() {
        Vec::new()
    } else {
        vec![Span::styled(
            format!("{} ▣", c.listed_panes().count()),
            Style::default().fg(th.dim),
        )]
    })
}

/// A branch with no directory of its own — an offer of one, and something
/// you can switch to or delete from where it stands.
pub(super) fn branch_item(name: &str, th: Theme) -> Item<'static> {
    Item::new(
        vec![
            status_dot(None, th),
            Span::styled("⌥ ", Style::default().fg(th.dim)),
            Span::styled(name.to_string(), Style::default().fg(th.muted)),
        ],
        vec![Span::styled("no checkout", Style::default().fg(th.dim))],
    )
}

/// A branch that only exists on a remote. Named as the remote names it,
/// because the point of the row is that it isn't here yet.
pub(super) fn remote_item(name: &str, th: Theme) -> Item<'static> {
    Item::new(
        vec![
            status_dot(None, th),
            Span::styled("⇣ ", Style::default().fg(th.dim)),
            Span::styled(name.to_string(), Style::default().fg(th.muted)),
        ],
        vec![Span::styled(
            "on the remote only",
            Style::default().fg(th.dim),
        )],
    )
}

/// The compact git summary on a checkout row: branch name (or `detached`),
/// commits ahead/behind upstream, and a dirty marker with its file count.
/// Each part carries its own color, so `clean` and `*3` read differently at
/// a glance instead of being one undifferentiated string.
/// [`git_spans`] with the branch name elided when it merely repeats the
/// row's own label. The ahead/behind/dirty markers always survive — those
/// are never implied by the name.
pub(super) fn git_spans_unless_branch_is(
    git: Option<&GitStatus>,
    name: &str,
    th: Theme,
) -> Vec<Span<'static>> {
    let redundant = git.and_then(|g| g.branch.as_deref()) == Some(name);
    git_spans(git, th)
        .into_iter()
        .filter(|s| !(redundant && s.content.trim() == name))
        .collect()
}

pub(super) fn git_spans(git: Option<&GitStatus>, th: Theme) -> Vec<Span<'static>> {
    let Some(g) = git else { return Vec::new() };
    let mut spans = vec![match &g.branch {
        Some(branch) => Span::styled(branch.clone(), Style::default().fg(th.muted)),
        None => Span::styled("detached".to_string(), Style::default().fg(th.dim)),
    }];
    if g.ahead > 0 {
        spans.push(Span::styled(
            format!("  ↑{}", g.ahead),
            Style::default().fg(th.ok),
        ));
    }
    if g.behind > 0 {
        spans.push(Span::styled(
            format!("  ↓{}", g.behind),
            Style::default().fg(th.warn),
        ));
    }
    // Spelled out rather than a `*n` sigil: the detail line has room, and
    // the count is the thing you act on.
    if g.dirty {
        spans.push(Span::styled(
            format!("  {}", plural(g.changed_files, "change")),
            Style::default().fg(th.warn),
        ));
    } else if g.branch.is_some() {
        spans.push(Span::styled("  clean", Style::default().fg(th.dim)));
    }
    spans
}

pub(super) fn status_word(status: PaneStatus) -> &'static str {
    match status {
        PaneStatus::Idle => "idle",
        PaneStatus::Working => "working",
        PaneStatus::Waiting => "needs you",
        PaneStatus::NeedsReview => "needs review",
        PaneStatus::Done => "done",
        PaneStatus::Failed => "failed",
        PaneStatus::Exited { .. } => "exited",
    }
}

/// The row's second line: which agent this is, then what it is saying.
///
/// The agent's own note when it has left one, because "needs you" without
/// saying what for still costs you a trip into the pane — which is the
/// whole thing this column exists to save.
///
/// The template leads because the name above it is the agent's own, and a
/// row renamed to "fixing the pty deadlock" otherwise stops saying which
/// CLI is in it. Dimmer than the note: it is how you tell two rows apart,
/// not what you came to read.
pub(super) fn pane_detail(p: &argus_protocol::PaneInfo, th: Theme) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    // Only once the agent has taken the name over — before that the row
    // already reads "opencode" and saying it twice is noise.
    if let Some(template) = p.template.as_deref().filter(|t| *t != p.title) {
        spans.push(Span::styled(
            format!("{template}  "),
            Style::default().fg(th.dim),
        ));
    }
    match p.note.as_deref().filter(|n| !n.is_empty()) {
        Some(note) => spans.push(Span::styled(
            note.to_string(),
            Style::default().fg(if p.status.needs_you() {
                th.err
            } else {
                th.muted
            }),
        )),
        None => spans.push(Span::styled(
            status_word(p.status),
            Style::default().fg(th.dim),
        )),
    }
    spans
}

/// One agent running underneath a pane, as a row of its own beneath its
/// parent (DESIGN.md §8b). Indented and unbolded so the column still reads
/// as a list of panes: a child is something happening in a pane, not
/// somewhere else to go — selecting its row selects the pane it runs in.
pub(super) fn child_item(c: &ChildAgentInfo, th: Theme) -> Item<'static> {
    Item::new(
        vec![
            Span::styled("  ⤷ ", Style::default().fg(th.dim)),
            status_dot(Some(c.status), th),
            Span::styled(c.label.clone(), Style::default().fg(th.muted)),
        ],
        vec![Span::styled(
            match c.note.as_deref().filter(|n| !n.is_empty()) {
                Some(note) => format!("    {note}"),
                None => format!("    {}", status_word(c.status)),
            },
            Style::default().fg(if c.status.needs_you() { th.err } else { th.dim }),
        )],
    )
}

/// Which pane each row of the panes column belongs to: a pane's own row,
/// then one row per child listed under it. Shared with `app` so a click
/// lands on the same pane the renderer drew there.
pub fn pane_row_owners(app: &App) -> Vec<PaneLocation> {
    app.pane_column_locations()
        .into_iter()
        .flat_map(|location| {
            let children = app
                .pane_at(location)
                .map(|pane| pane.children.len())
                .unwrap_or(0);
            std::iter::repeat_n(location, 1 + children)
        })
        .collect()
}

pub(super) fn exit_note(status: PaneStatus) -> String {
    match status {
        PaneStatus::Exited { code: Some(0) } => String::new(),
        PaneStatus::Exited { code: Some(c) } => format!("  exit {c}"),
        PaneStatus::Exited { code: None } => "  killed".to_string(),
        _ => String::new(),
    }
}

pub(super) fn worst_pane_status(c: &argus_protocol::CheckoutInfo) -> Option<PaneStatus> {
    c.listed_panes()
        .flat_map(|p| std::iter::once(p.status).chain(p.children.iter().map(|child| child.status)))
        .max_by_key(|s| s.urgency())
}

/// Shape carries the state signal (§8b); color reinforces it but is never
/// the only distinction. Outlined shapes mark idle or cleanly exited work.
pub(super) fn status_dot(status: Option<PaneStatus>, th: Theme) -> Span<'static> {
    let (glyph, color) = match status {
        None => ("· ", th.dim),
        Some(PaneStatus::Idle) => ("○ ", th.ok),
        Some(PaneStatus::Working) => ("● ", th.warn),
        Some(PaneStatus::Waiting) => ("▲ ", th.err),
        Some(PaneStatus::NeedsReview) => ("◆ ", th.err),
        Some(PaneStatus::Done) => ("✓ ", th.ok),
        // Still running, unlike an exit, so it is a block rather than a cross.
        Some(PaneStatus::Failed) => ("■ ", th.err),
        Some(PaneStatus::Exited { code: Some(0) }) => ("□ ", th.dim),
        Some(PaneStatus::Exited { .. }) => ("✗ ", th.err),
    };
    Span::styled(glyph, Style::default().fg(color))
}
