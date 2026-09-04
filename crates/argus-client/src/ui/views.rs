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
    let split = feature_column_width(area.width);
    let features = Rect {
        width: split,
        ..area
    };
    let board = Rect {
        x: area.x + split,
        width: area.width.saturating_sub(split),
        ..area
    };
    render_feature_column(f, app, features, th);
    render_feature_board(f, app, board, th);
}

/// How wide the feature column gets. Fixed rather than proportional past a
/// point: a feature is a short title, and a column that grew with the
/// terminal would spend the width the tree needs for its branches.
fn feature_column_width(total: u16) -> u16 {
    const IDEAL: u16 = 30;
    (total / 3).clamp(0, IDEAL)
}

/// The features of the project, which is the scope the tree beside it is
/// read at. Left of the tree and always drawn, because a board with no way
/// to see what else there is answers only the question you already asked.
fn render_feature_column(f: &mut Frame, app: &mut App, area: Rect, th: Theme) {
    let focused = app.board_on_features;
    let title = match app.board.as_ref().map(|b| b.name.clone()) {
        Some(name) => format!("features · {name}"),
        None => "features".to_string(),
    };
    let block = panel_block(&title, focused, th, area.width);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let rows = app.feature_rows();
    let per_row = ROW_HEIGHT as usize;
    let visible = (inner.height as usize) / per_row.max(1);
    let first = scrolled_to_show(0, Some(app.board_feature_sel), visible, rows.len());
    app.layout.features = Panel {
        outer: area,
        inner,
        first,
    };
    if rows.is_empty() {
        f.render_widget(
            Paragraph::new(Span::styled(
                "no features yet",
                Style::default().fg(th.dim),
            )),
            inner,
        );
        return;
    }
    let mut lines = Vec::new();
    for (index, row) in rows.iter().enumerate().skip(first).take(visible) {
        let selected = index == app.board_feature_sel;
        let name_style = match (selected, row.slug.is_some()) {
            (_, false) => Style::default().fg(th.dim),
            (true, _) => Style::default().fg(th.text).add_modifier(Modifier::BOLD),
            (false, _) => Style::default().fg(th.text),
        };
        lines.push(Line::from(vec![
            Span::styled(
                if selected { MARKER } else { GUTTER },
                Style::default().fg(th.accent),
            ),
            Span::styled(row.title.clone(), name_style),
        ]));
        let detail = match (&row.branch, row.decisions) {
            (Some(branch), n) => format!("{branch} · {n} decided"),
            (None, n) => format!("{n} decided"),
        };
        lines.push(Line::from(Span::styled(
            format!("   {detail}"),
            Style::default().fg(th.dim),
        )));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

fn render_feature_board(f: &mut Frame, app: &mut App, area: Rect, th: Theme) {
    let title = match app.current_feature_row() {
        Some(row) => format!("decisions · {}", row.title),
        None => "decisions".to_string(),
    };
    let block = panel_block(&title, !app.board_on_features, th, area.width);
    let inner = block.inner(area);
    f.render_widget(block, area);

    // The brief above the tree, which is the order `argus-hook feature`
    // prints them in and the order they are read: a decision without what
    // the feature is for explains half of itself. Bounded so a long brief
    // cannot crowd out the reasoning it introduces — the rest is one `e`
    // away, in the editor.
    let inner = match brief_of(app) {
        Some(brief) => {
            let height = brief_height(&brief, inner);
            let rows = Rect { height, ..inner };
            f.render_widget(
                Paragraph::new(brief)
                    .wrap(Wrap { trim: true })
                    .style(Style::default().fg(th.dim)),
                rows,
            );
            Rect {
                y: inner.y + height,
                height: inner.height.saturating_sub(height),
                ..inner
            }
        }
        None => inner,
    };

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
    for (index, row) in rows.iter().enumerate().skip(first).take(visible) {
        push_board_row(&mut lines, row, index == app.board_sel, th);
    }
    f.render_widget(Paragraph::new(lines), inner);
}

fn push_board_row(
    lines: &mut Vec<Line<'static>>,
    row: &argus_protocol::DecisionTreeRow<'_>,
    selected: bool,
    th: Theme,
) {
    let decision = row.decision;
    let name_style = match (selected, decision.superseded()) {
        (_, true) => Style::default().fg(th.dim),
        (true, false) => Style::default().fg(th.text).add_modifier(Modifier::BOLD),
        (false, false) => Style::default().fg(th.text),
    };
    let mut name = vec![
        Span::styled(
            if selected { MARKER } else { GUTTER },
            Style::default().fg(th.accent),
        ),
        Span::styled(
            format!("{}#{} ", board_branch(row), decision.id),
            Style::default().fg(th.dim),
        ),
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
        format!(" {}  {}", board_continuation(row), board_detail(decision)),
        Style::default().fg(th.dim),
    )));
}

fn board_branch(row: &argus_protocol::DecisionTreeRow<'_>) -> String {
    if row.depth == 0 {
        return String::new();
    }
    let mut branch = board_ancestor_guides(row, 1);
    branch.push_str(if row.has_next_sibling { "├─ " } else { "└─ " });
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
fn brief_of(app: &App) -> Option<String> {
    let brief = app.selected_feature()?.body.trim().to_string();
    (!brief.is_empty()).then_some(brief)
}

/// How many rows the brief may have: a third of the panel, and never more
/// than the wrapped text actually needs.
fn brief_height(brief: &str, inner: Rect) -> u16 {
    let width = inner.width.max(1) as usize;
    let wrapped = brief
        .lines()
        .map(|line| line.chars().count().max(1).div_ceil(width))
        .sum::<usize>() as u16;
    // Plus a blank row under it, so the tree does not begin mid-paragraph.
    (wrapped + 1).min(inner.height / 3)
}

/// A board nobody has written to yet. It says what it is for rather than
/// nothing at all, because a blank card on a tab somebody just pressed
/// reads as a bug.
fn render_empty_board(f: &mut Frame, inner: Rect, th: Theme) {
    f.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                "nothing decided under this feature yet",
                Style::default().fg(th.text),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Work is scoped to a feature, and agents record a decision under it when \
                 they choose between real options while planning: what was chosen, what \
                 it was chosen over, and what forced it. Each one hangs off the decision \
                 that constrained it, so what accumulates is a reference tree for this \
                 feature rather than a log of the whole project.",
                Style::default().fg(th.dim),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "the board arrives on its own as it is written · 1 goes back to the spine",
                Style::default().fg(th.dim),
            )),
        ])
        .wrap(Wrap { trim: true }),
        inner,
    );
}

/// The board: every feature of the project in a column by state.
///
/// Equal columns rather than proportional. What is worth width here is
/// whichever column is full, and that changes hour to hour — a layout that
/// tracked it would move the columns around under a reader who is using
/// their position to find them.
pub(super) fn render_board(f: &mut Frame, app: &mut App, area: Rect, th: Theme) {
    let (area, prompt) = split_off_prompt(area, app.line.is_some());
    let states = argus_protocol::FeatureState::ALL;
    let each = area.width / states.len() as u16;
    for (index, state) in states.into_iter().enumerate() {
        let x = area.x + each * index as u16;
        // The last column takes the remainder, so a width that does not
        // divide by five leaves no unpainted strip at the edge.
        let width = if index + 1 == states.len() {
            area.width.saturating_sub(each * index as u16)
        } else {
            each
        };
        render_board_column(f, app, index, state, Rect { x, width, ..area }, th);
    }
    render_line(f, app, prompt, th);
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

fn render_board_column(
    f: &mut Frame,
    app: &mut App,
    index: usize,
    state: argus_protocol::FeatureState,
    area: Rect,
    th: Theme,
) {
    let focused = index == app.board_column;
    let cards: Vec<_> = app
        .column_features(state)
        .into_iter()
        .cloned()
        .collect();
    let title = format!("{state} · {}", cards.len());
    let block = panel_block(&title, focused, th, area.width);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let per_row = ROW_HEIGHT as usize;
    let visible = (inner.height as usize) / per_row.max(1);
    let selected = focused.then_some(app.board_card);
    let first = scrolled_to_show(0, selected, visible, cards.len());
    app.layout.board_columns[index] = Panel {
        outer: area,
        inner,
        first,
    };
    if cards.is_empty() {
        return;
    }

    let mut lines = Vec::new();
    for (row, feature) in cards.iter().enumerate().skip(first).take(visible) {
        let on = focused && row == app.board_card;
        lines.push(Line::from(vec![
            Span::styled(
                if on { MARKER } else { GUTTER },
                Style::default().fg(th.accent),
            ),
            Span::styled(
                feature.title.clone(),
                if on {
                    Style::default().fg(th.text).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(th.text)
                },
            ),
        ]));
        lines.push(Line::from(Span::styled(
            format!("   {}", card_detail(feature)),
            Style::default().fg(th.dim),
        )));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

/// A card's second line: whatever the column it is in leaves unsaid.
///
/// A blocked card says why, a submitted one says what was offered, and one
/// nobody has picked up says the branch it was cut on — each column's
/// second line answers the question that column raises.
fn card_detail(feature: &argus_protocol::Feature) -> String {
    use argus_protocol::FeatureState::*;
    let fallback = || {
        feature
            .origin_branch
            .clone()
            .unwrap_or_else(|| "no branch recorded".to_string())
    };
    match feature.state {
        Blocked => feature.blocker.clone().unwrap_or_else(|| "blocked, reason not given".into()),
        Submitted => feature
            .evidence
            .clone()
            .unwrap_or_else(|| "submitted with no evidence".into()),
        Active => match &feature.claimed_by {
            Some(session) => format!("{} · {session}", fallback()),
            None => fallback(),
        },
        Proposed | Done => fallback(),
    }
}

/// One feature's tasks, in columns of their own.
///
/// The same shape as the feature board a level up. Going into a card
/// should not change how the screen works — only what is on it.
pub(super) fn render_tasks(f: &mut Frame, app: &mut App, area: Rect, th: Theme) {
    // The typed line takes a row off the bottom while it is up, rather
    // than floating over the cards: what you are writing and what is
    // already there have to be readable at the same time.
    let (area, prompt) = match app.line.is_some() {
        true => (
            Rect {
                height: area.height.saturating_sub(1),
                ..area
            },
            Some(Rect {
                y: area.y + area.height.saturating_sub(1),
                height: 1,
                ..area
            }),
        ),
        false => (area, None),
    };
    let states = argus_protocol::TaskState::ALL;
    let each = area.width / states.len() as u16;
    for (index, state) in states.into_iter().enumerate() {
        let x = area.x + each * index as u16;
        let width = if index + 1 == states.len() {
            area.width.saturating_sub(each * index as u16)
        } else {
            each
        };
        render_task_column(f, app, index, state, Rect { x, width, ..area }, th);
    }
    render_line(f, app, prompt, th);
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
            Span::styled(format!(" {} ", input.label()), Style::default().fg(th.accent)),
            Span::styled(input.text.clone(), Style::default().fg(th.text)),
            Span::styled("_", Style::default().fg(th.accent)),
        ])),
        row,
    );
}

fn render_task_column(
    f: &mut Frame,
    app: &mut App,
    index: usize,
    state: argus_protocol::TaskState,
    area: Rect,
    th: Theme,
) {
    let focused = index == app.task_column;
    let tasks: Vec<_> = app.task_column(state).into_iter().cloned().collect();
    // The feature is named on the first column rather than in a heading of
    // its own: a row spent on a title is a row of cards not drawn, and the
    // board you came from already said which card you went into.
    let title = if index == 0 {
        match app.tasks.as_ref().and_then(|l| l.feature.as_deref()) {
            Some(feature) => format!("{state} · {feature}"),
            None => format!("{state} · no feature"),
        }
    } else {
        format!("{state} · {}", tasks.len())
    };
    let block = panel_block(&title, focused, th, area.width);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let per_row = ROW_HEIGHT as usize;
    let visible = (inner.height as usize) / per_row.max(1);
    let selected = focused.then_some(app.task_card);
    let first = scrolled_to_show(0, selected, visible, tasks.len());
    app.layout.task_columns[index] = Panel {
        outer: area,
        inner,
        first,
    };
    if tasks.is_empty() {
        if index == 0 && app.tasks.as_ref().is_some_and(|l| l.tasks.is_empty()) {
            f.render_widget(
                Paragraph::new(Span::styled(
                    "nothing to do here yet",
                    Style::default().fg(th.dim),
                )),
                inner,
            );
        }
        return;
    }

    let mut lines = Vec::new();
    for (row, task) in tasks.iter().enumerate().skip(first).take(visible) {
        let on = focused && row == app.task_card;
        lines.push(Line::from(vec![
            Span::styled(
                if on { MARKER } else { GUTTER },
                Style::default().fg(th.accent),
            ),
            Span::styled(
                task.title.clone(),
                if on {
                    Style::default().fg(th.text).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(th.text)
                },
            ),
        ]));
        lines.push(Line::from(Span::styled(
            format!("   {}", task_detail(task)),
            Style::default().fg(th.dim),
        )));
    }
    f.render_widget(Paragraph::new(lines), inner);
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
