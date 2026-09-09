//! The tab strip, and the views that are not the spine.
//!
//! The strip is one row at the very top of the page, above every card and
//! every overlay's frame. It costs a row on purpose: a view you cannot see
//! the existence of is a view nobody opens, and the numbers on the tabs
//! are the whole of their documentation.

use super::*;

use crate::app::{FeaturePanel, View};

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

/// How many rows a panel can show, once the one being read has been paid
/// for.
///
/// The selected row is as tall as its own wrapped text, so it is counted
/// in full first and the rest of the height is divided among the others.
/// Rounding up instead — which is right when nothing is expanded, since a
/// half-drawn row is still a row you can read the name off — is what let
/// an expanded row start on the last line and run off the bottom, which is
/// the one thing the allowance exists to prevent.
fn visible_rows(height: u16, grown: usize) -> usize {
    let per_row = (ROW_HEIGHT as usize).max(1);
    let height = height as usize;
    if grown == 0 {
        return height.div_ceil(per_row);
    }
    1 + height.saturating_sub(grown + per_row) / per_row
}

/// How much of a row's width the tree guides may take before the text is
/// what suffers. A decision nested past this is drawn at the last indent
/// that still leaves room to read it.
const MAX_BOARD_INDENT: usize = 24;

/// The feature view: the selected repository branch's features, and the one under the cursor
/// read whole.
///
/// One view rather than three. A brief, what is left to do under it and
/// why it has the shape it does are one object, and splitting them across
/// three tabs gave each a selection of its own — which is how pressing the
/// tasks tab came to show a different feature's tasks than the ones you
/// were reading about.
pub(super) fn render_feature(f: &mut Frame, app: &mut App, area: Rect, th: Theme) {
    let (area, prompt) = split_off_prompt(area, app.line.is_some());
    let split = feature_column_width(area.width);
    render_feature_column(
        f,
        app,
        Rect {
            width: split,
            ..area
        },
        th,
    );
    render_selected_feature(
        f,
        app,
        Rect {
            x: area.x + split,
            width: area.width.saturating_sub(split),
            ..area
        },
        th,
    );
    render_line(f, app, prompt, th);
}

/// How wide the feature column gets. Fixed rather than proportional past a
/// point: a feature is a short title and a line about what is happening to
/// it, and a column that grew with the terminal would spend the width the
/// tree beside it needs for its branches.
fn feature_column_width(total: u16) -> u16 {
    const IDEAL: u16 = 34;
    (total / 3).clamp(0, IDEAL)
}

/// The features of the project, which is the scope everything to the right
/// is read at.
fn render_feature_column(f: &mut Frame, app: &mut App, area: Rect, th: Theme) {
    let focused = app.panel == FeaturePanel::Features;
    let title = match app.board.as_ref().map(|b| b.name.clone()) {
        Some(name) => format!("features · {name}"),
        None => "features".to_string(),
    };
    let block = panel_block(&title, focused, th, area.width);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let rows = app.feature_rows();
    let grown = grown_by(
        rows.get(app.feature_sel)
            .map(|row| (row.title.clone(), row.detail.clone())),
        th,
        inner.width,
    );
    let visible = visible_rows(inner.height, grown);
    let first = scrolled_to_show(0, Some(app.feature_sel), visible, rows.len());
    app.layout.features = Panel {
        outer: area,
        inner,
        first,
    };
    if rows.is_empty() {
        f.render_widget(
            Paragraph::new(Span::styled(
                "no features yet — a opens one",
                Style::default().fg(th.dim),
            )),
            inner,
        );
        return;
    }
    let mut lines = Vec::new();
    for (index, row) in rows.iter().enumerate().skip(first) {
        if lines.len() >= inner.height as usize {
            break;
        }
        let selected = index == app.feature_sel;
        // An accepted feature and the unfiled row both recede: neither is
        // work anybody is going to pick up, and a list where everything
        // reads at one weight is a list nothing stands out of.
        let title_style = match (selected, row.slug.is_some() && !row.done) {
            (_, false) => Style::default().fg(th.dim),
            (true, _) => Style::default().fg(th.text).add_modifier(Modifier::BOLD),
            (false, _) => Style::default().fg(th.text),
        };
        // The one line in the list that is not about the feature but about
        // what is happening to it, so a row that has stopped for somebody
        // is coloured like the pane that stopped.
        let detail_style = match row.attention {
            Some(argus_protocol::PaneStatus::Failed) => Style::default().fg(th.err),
            Some(_) => Style::default().fg(th.warn),
            None => Style::default().fg(th.dim),
        };
        push_card_row(
            &mut lines,
            Card {
                title: row.title.clone(),
                title_style,
                detail: row.detail.clone(),
                detail_style,
            },
            selected,
            th,
            inner.width,
        );
    }
    f.render_widget(Paragraph::new(lines), inner);
}

/// The selected feature, stacked: what it is for, what is left to do, and
/// why it has the shape it does.
///
/// That order because it is the order they are read, and the order
/// `argus-hook feature` prints them in. The brief takes only what it needs
/// and never more than a third — the rest of it is one `e` away, in the
/// editor — because a long brief must not crowd out the work it
/// introduces.
fn render_selected_feature(f: &mut Frame, app: &mut App, area: Rect, th: Theme) {
    let brief = brief_of(app);
    let brief_rows = brief
        .as_deref()
        .map(|brief| brief_height(brief, area))
        .unwrap_or(0);
    if let (Some(brief), true) = (brief.as_deref(), brief_rows > 0) {
        render_brief(
            f,
            app,
            brief,
            Rect {
                height: brief_rows,
                ..area
            },
            th,
        );
    }
    let rest = Rect {
        y: area.y + brief_rows,
        height: area.height.saturating_sub(brief_rows),
        ..area
    };
    let tasks_rows = tasks_height(app.feature_tasks().len(), rest.height);
    render_tasks(
        f,
        app,
        Rect {
            height: tasks_rows,
            ..rest
        },
        th,
    );
    render_decisions(
        f,
        app,
        Rect {
            y: rest.y + tasks_rows,
            height: rest.height.saturating_sub(tasks_rows),
            ..rest
        },
        th,
    );
}

/// How much of the space under the brief the tasks take.
///
/// What they need, bounded so neither panel can squeeze the other out: a
/// feature with thirty tasks must still show why it has the shape it does,
/// and one with none must still say so rather than being a border.
fn tasks_height(tasks: usize, available: u16) -> u16 {
    const CHROME: u16 = 3;
    const FLOOR: u16 = 5;
    if available < FLOOR * 2 {
        return available / 2;
    }
    let wanted = tasks as u16 * ROW_HEIGHT + CHROME;
    wanted.clamp(FLOOR, available.saturating_sub(FLOOR))
}

/// What the feature is for. Never focused — it is prose rather than a
/// list, and `e` from the feature column opens it in the note editor,
/// which is where prose is corrected.
fn render_brief(f: &mut Frame, app: &App, brief: &str, area: Rect, th: Theme) {
    let title = match app.current_feature_row() {
        Some(row) => format!("brief · {}", row.title),
        None => "brief".to_string(),
    };
    let block = panel_block(&title, false, th, area.width);
    let inner = block.inner(area);
    f.render_widget(block, area);
    // Marked up, but quieter than the work it introduces: the brief is
    // read once for context and then skimmed past, so its headings earn
    // their weight while the prose stays background.
    let body: Vec<Line> = brief
        .lines()
        .map(|line| {
            Line::from(crate::ui::prose::prose_spans(
                line,
                Style::default().fg(th.dim),
                th,
            ))
        })
        .collect();
    f.render_widget(Paragraph::new(body).wrap(Wrap { trim: true }), inner);
}

/// What is left to do under the feature, as one list.
///
/// One list and not three columns. A task's state is a mark on its row, so
/// the order stays what a person set it to — what to do first — rather
/// than being spent saying the same thing the columns already said. It is
/// also the shape that lets a reader see the feature and its work at once,
/// which three columns of cards never left room for.
fn render_tasks(f: &mut Frame, app: &mut App, area: Rect, th: Theme) {
    let focused = app.panel == FeaturePanel::Tasks;
    let tasks: Vec<_> = app.feature_tasks().to_vec();
    // Counted from the list on screen when it has arrived, and from the
    // feature row until then. Taking it only from the row would let the
    // heading say 3/7 over a list of four, which is the kind of
    // disagreement two sources of one number always end in.
    let counts = match app
        .tasks
        .as_ref()
        .map(|list| list.feature == app.feature_slug())
    {
        Some(true) => {
            tasks
                .iter()
                .fold(argus_protocol::TaskCounts::default(), |mut counts, task| {
                    counts.add(task.state);
                    counts
                })
        }
        _ => app.selected_feature().map(|f| f.tasks).unwrap_or_default(),
    };
    let title = match (app.feature_slug().is_some(), counts.total()) {
        (false, _) | (true, 0) => "tasks".to_string(),
        (true, total) => format!("tasks · {}/{}", counts.done, total),
    };
    let block = panel_block(&title, focused, th, area.width);
    let inner = block.inner(area);
    f.render_widget(block, area);

    // Drawn in every panel, focused or not: the selections are how a
    // reader traces which feature, which task and which decision they are
    // looking at, and a panel that forgets its own the moment the keys
    // leave it makes crossing back a hunt.
    let selected = Some(app.task_sel);
    let grown = selected
        .and_then(|i| tasks.get(i))
        .map(|task| task_grown_by(task, th, inner.width))
        .unwrap_or(0);
    let visible = visible_rows(inner.height, grown);
    let first = scrolled_to_show(0, selected, visible, tasks.len());
    app.layout.feature_tasks = Panel {
        outer: area,
        inner,
        first,
    };
    if tasks.is_empty() {
        let empty = match app.feature_slug().is_some() {
            true => "nothing to do here yet — a adds a task",
            false => "no feature selected",
        };
        f.render_widget(
            Paragraph::new(Span::styled(empty, Style::default().fg(th.dim))),
            inner,
        );
        return;
    }

    let mut lines = Vec::new();
    for (row, task) in tasks.iter().enumerate().skip(first) {
        if lines.len() >= inner.height as usize {
            break;
        }
        push_task_row(&mut lines, task, row == app.task_sel, th, inner.width);
    }
    f.render_widget(Paragraph::new(lines), inner);
}

/// The mark that says how far along a task is.
///
/// A glyph rather than a column, so the list keeps the order a person put
/// it in. Distinct shapes rather than colour alone: a state you can only
/// see if you can tell two greys apart is a state half the readers cannot
/// see at all.
fn task_mark(state: argus_protocol::TaskState) -> &'static str {
    match state {
        argus_protocol::TaskState::Todo => "○",
        argus_protocol::TaskState::Doing => "▶",
        argus_protocol::TaskState::Done => "✓",
    }
}

fn push_task_row(
    lines: &mut Vec<Line<'static>>,
    task: &argus_protocol::Task,
    selected: bool,
    th: Theme,
    width: u16,
) {
    use argus_protocol::TaskState;
    // The marker, the state glyph, and a space: what the wrapped title and
    // the detail line both hang to.
    const HANG: usize = 3;
    let mark_style = match task.state {
        TaskState::Todo => Style::default().fg(th.muted),
        TaskState::Doing => Style::default().fg(th.accent),
        TaskState::Done => Style::default().fg(th.ok),
    };
    // A finished task recedes rather than leaving: what has been done is
    // most of what says how far along the feature is, but it is not what
    // anybody is going to pick up next.
    let title_style = match (selected, task.state) {
        (_, TaskState::Done) => Style::default().fg(th.dim),
        (true, _) => Style::default().fg(th.text).add_modifier(Modifier::BOLD),
        (false, _) => Style::default().fg(th.text),
    };
    lines.extend(text_rows(
        vec![
            Span::styled(
                if selected { MARKER } else { GUTTER },
                Style::default().fg(th.accent),
            ),
            Span::styled(task_mark(task.state), mark_style),
            Span::raw(" "),
        ],
        HANG,
        task.title.clone(),
        title_style,
        width,
        selected,
    ));
    lines.extend(text_rows(
        vec![Span::raw(" ".repeat(HANG))],
        HANG,
        task_detail(task),
        Style::default().fg(th.dim),
        width,
        selected,
    ));
    if selected {
        if let Some(body) = task.body.as_deref().filter(|body| !body.trim().is_empty()) {
            lines.extend(text_rows(
                vec![Span::raw(" ".repeat(HANG))],
                HANG,
                body.to_string(),
                Style::default().fg(th.muted),
                width,
                true,
            ));
        }
    }
}

fn task_grown_by(task: &argus_protocol::Task, th: Theme, width: u16) -> usize {
    let mut lines = Vec::new();
    push_task_row(&mut lines, task, true, th, width);
    lines.len().saturating_sub(ROW_HEIGHT as usize)
}

/// A task's second line: the number an agent names it by, whoever has it,
/// and the tracker key it came from — which is the whole of what Argus
/// knows about wherever it came from.
fn task_detail(task: &argus_protocol::Task) -> String {
    let mut parts = vec![format!("#{}", task.id)];
    if let Some(key) = &task.external {
        parts.push(key.clone());
    }
    if let Some(session) = &task.claimed_by {
        parts.push(session.clone());
    }
    parts.join(" · ")
}

/// The decision board, drawn as the tree it is.
///
/// Two lines per decision, like every other list in Argus: what was
/// chosen, then the dimmer line of what it was chosen over and what forced
/// it. A superseded decision keeps its place and goes dim — the road not
/// taken is most of what a reader came for.
fn render_decisions(f: &mut Frame, app: &mut App, area: Rect, th: Theme) {
    let focused = app.panel == FeaturePanel::Decisions;
    let count = app.board_rows().len();
    let title = match count {
        0 => "decisions".to_string(),
        n => format!("decisions · {n}"),
    };
    let block = panel_block(&title, focused, th, area.width);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let rows_now = app.board_rows();
    let per_row = ROW_HEIGHT as usize;
    // The expanded row costs the height of its own wrapped text, so the
    // window has that much less to give the rows around it. Without this
    // the selection lands at the bottom and then wraps off the card.
    let grown = rows_now
        .get(app.decision_sel)
        .map(|row| {
            let mut lines = Vec::new();
            push_board_row(&mut lines, row, true, th, inner.width);
            lines.len().saturating_sub(per_row)
        })
        .unwrap_or(0);
    let visible = visible_rows(inner.height, grown);
    let selected = Some(app.decision_sel);
    // A scrolled tree's top row is `first`, not row zero, and a click has
    // to resolve against the rows that were actually drawn.
    let first = scrolled_to_show(0, selected, visible, count);
    app.layout.feature_decisions = Panel {
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
    for (index, row) in rows.iter().enumerate().skip(first) {
        // The selected row is as tall as its text, so the window is filled
        // by height rather than by a count of rows.
        if lines.len() >= inner.height as usize {
            break;
        }
        push_board_row(&mut lines, row, selected == Some(index), th, inner.width);
    }
    f.render_widget(Paragraph::new(lines), inner);
}

/// `text` laid out after `prefix`, wrapped over as many lines as it needs
/// when `expand`, and left to clip at the card's edge when not.
///
/// Only the row the cursor is on expands. A list where every row grows to
/// its content cannot be scanned — the fixed two lines are what let the
/// eye run down a column — but a row clipped with no way to see the rest
/// is a row lying about what is in it. Expanding the one row somebody is
/// actually reading is what makes the list both scannable and complete.
///
/// Continuation lines are indented to `hang`, so wrapped prose sits under
/// the text it belongs to rather than under the marker or the tree guides.
fn text_rows(
    prefix: Vec<Span<'static>>,
    hang: usize,
    text: String,
    style: Style,
    width: u16,
    expand: bool,
) -> Vec<Line<'static>> {
    let used: usize = prefix.iter().map(Span::width).sum();
    if !expand {
        let mut spans = prefix;
        spans.push(Span::styled(text, style));
        return vec![Line::from(spans)];
    }
    // Every line is wrapped to the widest indent any of them carries, not
    // to the first line's own. Wrapping to the prefix and then indenting
    // the continuations by more than that is how they come to overrun the
    // card and lose their last character to the clip.
    let room = (width as usize).saturating_sub(used.max(hang)).max(1);
    let mut rows = wrap(&text, room as u16).into_iter();
    let first = rows.next().unwrap_or_default();
    let mut spans = prefix;
    spans.push(Span::styled(first, style));
    let mut lines = vec![Line::from(spans)];
    for row in rows {
        lines.push(Line::from(vec![
            Span::raw(" ".repeat(hang)),
            Span::styled(row, style),
        ]));
    }
    lines
}

fn push_board_row(
    lines: &mut Vec<Line<'static>>,
    row: &argus_protocol::DecisionTreeRow<'_>,
    selected: bool,
    th: Theme,
    width: u16,
) {
    let decision = row.decision;
    let name_style = match (selected, decision.superseded()) {
        (_, true) => Style::default().fg(th.dim),
        (true, false) => Style::default().fg(th.text).add_modifier(Modifier::BOLD),
        (false, false) => Style::default().fg(th.text),
    };
    let branch = board_branch(row);
    let prefix = vec![
        Span::styled(
            if selected { MARKER } else { GUTTER },
            Style::default().fg(th.accent),
        ),
        Span::styled(
            format!("{branch}#{} ", decision.id),
            Style::default().fg(th.dim),
        ),
    ];
    // Wrapped text hangs under the choice rather than under the tree
    // guides, so a deep decision still reads as one paragraph.
    let hang = prefix.iter().map(Span::width).sum();
    let mut chose = decision.chose.clone();
    if let Some(by) = decision.superseded_by {
        chose.push_str(&format!("  superseded by #{by}"));
    }
    lines.extend(text_rows(prefix, hang, chose, name_style, width, selected));

    let continuation = format!(" {}  ", board_continuation(row));
    let hang = continuation.chars().count();
    lines.extend(text_rows(
        vec![Span::styled(continuation, Style::default().fg(th.dim))],
        hang,
        board_detail(decision),
        Style::default().fg(th.dim),
        width,
        selected,
    ));
}

fn board_branch(row: &argus_protocol::DecisionTreeRow<'_>) -> String {
    if row.depth == 0 {
        return String::new();
    }
    let mut branch = board_ancestor_guides(row, 1);
    branch.push_str(if row.has_next_sibling {
        "├─ "
    } else {
        "└─ "
    });
    branch
}

fn board_continuation(row: &argus_protocol::DecisionTreeRow<'_>) -> String {
    let reserved = usize::from(row.depth > 0) + usize::from(row.has_children);
    let mut continuation = board_ancestor_guides(row, reserved);
    if row.depth > 0 {
        continuation.push_str(if row.has_next_sibling { "│  " } else { "   " });
    }
    continuation.push_str(if row.has_children { "│  " } else { "   " });
    continuation
}

fn board_ancestor_guides(
    row: &argus_protocol::DecisionTreeRow<'_>,
    reserved_slots: usize,
) -> String {
    let slots = (MAX_BOARD_INDENT / 3).saturating_sub(reserved_slots);
    let first = row.ancestor_continuations.len().saturating_sub(slots);
    row.ancestor_continuations[first..]
        .iter()
        .map(|continues| if *continues { "│  " } else { "   " })
        .collect()
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

/// The brief of the feature the decision view is on, if it has one.
/// The brief of the selected feature, if it has one.
fn brief_of(app: &App) -> Option<String> {
    let brief = app.selected_feature()?.body.trim().to_string();
    (!brief.is_empty()).then_some(brief)
}

/// How tall the brief's panel is: what the wrapped text needs plus its
/// own chrome, and never more than a third of the space.
///
/// Zero when there is not enough room for a border and a line of prose,
/// which is what keeps a short terminal from spending three rows on a
/// panel that can show nothing.
fn brief_height(brief: &str, area: Rect) -> u16 {
    // Two borders and the block's top padding.
    const CHROME: u16 = 3;
    let width = area.width.saturating_sub(4).max(1) as usize;
    let wrapped = brief
        .lines()
        .map(|line| line.chars().count().max(1).div_ceil(width))
        .sum::<usize>() as u16;
    let room = area.height / 3;
    if room < CHROME + 1 {
        return 0;
    }
    (wrapped + CHROME).min(room)
}

/// A feature nobody has decided anything under yet. It says what the tree
/// is for rather than nothing at all, because a blank panel reads as a bug.
fn render_empty_board(f: &mut Frame, inner: Rect, th: Theme) {
    if inner.height < 4 {
        f.render_widget(
            Paragraph::new(Span::styled(
                "nothing decided here yet",
                Style::default().fg(th.dim),
            )),
            inner,
        );
        return;
    }
    f.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                "nothing decided under this feature yet",
                Style::default().fg(th.text),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Agents record a decision here when they choose between real options \
                 while planning: what was chosen, what it was chosen over, and what \
                 forced it. Each hangs off the decision that constrained it, so what \
                 accumulates is a reference for this feature rather than a log.",
                Style::default().fg(th.dim),
            )),
        ])
        .wrap(Wrap { trim: true }),
        inner,
    );
}

/// Takes the bottom row for a typed line, when there is one.
fn split_off_prompt(area: Rect, typing: bool) -> (Rect, Option<Rect>) {
    if !typing {
        return (area, None);
    }
    (
        Rect {
            height: area.height.saturating_sub(1),
            ..area
        },
        Some(Rect {
            y: area.y + area.height.saturating_sub(1),
            height: 1,
            ..area
        }),
    )
}

/// One card of the board or tasks columns: its title, and the line under
/// it saying whatever the column it sits in leaves unsaid.
///
/// Shared because the two boards are the same object drawn twice, and a
/// card that wraps in one and clips in the other is the kind of difference
/// nobody decided on.
pub(super) struct Card {
    pub title: String,
    pub title_style: Style,
    /// The line under the name, saying whatever the list it is in leaves
    /// unsaid — and styled by the caller, because on the feature list it
    /// is about the panes rather than about the row.
    pub detail: String,
    pub detail_style: Style,
}

fn push_card_row(
    lines: &mut Vec<Line<'static>>,
    card: Card,
    selected: bool,
    th: Theme,
    width: u16,
) {
    let Card {
        title,
        title_style,
        detail,
        detail_style,
    } = card;
    // The detail line's indent, which wrapped title text hangs to as well
    // so a long name reads as one block rather than as two rows.
    const HANG: usize = 3;
    lines.extend(text_rows(
        vec![Span::styled(
            if selected { MARKER } else { GUTTER },
            Style::default().fg(th.accent),
        )],
        HANG,
        title,
        title_style,
        width,
        selected,
    ));
    lines.extend(text_rows(
        vec![Span::raw(" ".repeat(HANG))],
        HANG,
        detail,
        detail_style,
        width,
        selected,
    ));
}

/// How many lines beyond the usual two the selected card will take once
/// its text is wrapped. The window has that much less to give the cards
/// around it; without allowing for it the selection is placed at the
/// bottom of the card and its own text then wraps off the end.
fn grown_by(selected: Option<(String, String)>, th: Theme, width: u16) -> usize {
    let Some((title, detail)) = selected else {
        return 0;
    };
    let mut lines = Vec::new();
    push_card_row(
        &mut lines,
        Card {
            title,
            title_style: Style::default(),
            detail,
            detail_style: Style::default(),
        },
        true,
        th,
        width,
    );
    lines.len().saturating_sub(ROW_HEIGHT as usize)
}

/// The line being typed, on a row of its own at the foot of a board.
///
/// A row taken off the bottom rather than a window floating over the
/// cards: what you are writing and what is already there have to be
/// readable at the same time.
fn render_line(f: &mut Frame, app: &App, prompt: Option<Rect>, th: Theme) {
    let (Some(row), Some(input)) = (prompt, app.line.as_ref()) else {
        return;
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!(" {} ", input.label()),
                Style::default().fg(th.accent),
            ),
            Span::styled(input.text.clone(), Style::default().fg(th.text)),
            Span::styled("_", Style::default().fg(th.accent)),
        ])),
        row,
    );
}
