//! An agent view's blocks as styled lines of a given width.
//!
//! The terminal draws the blocks that read well as text — sections, tables,
//! fields, lists, code, meters — and says plainly where it leaves one to the
//! desktop. Nothing here is interactive: a button or a selection in a block
//! is the desktop's, and what is drawn is what the agent read.
//!
//! Lines are made whole, before anything is scrolled, so the stage can tell
//! how tall a view is and show a window of it.

use argus_protocol::{
    Block, CheckItem, Chip, Column, Item, LogLevel, LogLine, RowItem, Stat, Tab, Tone, Value,
    MAX_BLOCK_DEPTH,
};

use super::agent_values::*;
use super::*;

/// Deeper than the contract allows is not drawn: a view the daemon let
/// through cannot reach it, and a recursion should not trust that.
const DEEPEST: usize = MAX_BLOCK_DEPTH + 2;

/// Blocks the terminal leaves to the desktop, named by what they are.
fn desktop_only(block: &Block) -> Option<&'static str> {
    Some(match block {
        Block::Chart { .. } => "chart",
        Block::Graph { .. } => "graph",
        Block::Matrix { .. } => "matrix",
        Block::Board { .. } => "board",
        Block::Timeline { .. } => "timeline",
        Block::Tree { .. } => "tree",
        Block::Diff { .. } => "diff",
        Block::Form { .. } => "form",
        Block::Diagram { .. } => "diagram",
        _ => return None,
    })
}

pub(super) fn block_lines(blocks: &[Block], width: usize, th: Theme) -> Vec<Line<'static>> {
    let mut out = Vec::new();
    stack(blocks, width.max(1), 0, th, &mut out);
    out
}

fn stack(blocks: &[Block], width: usize, depth: usize, th: Theme, out: &mut Vec<Line<'static>>) {
    for (at, block) in blocks.iter().enumerate() {
        if at > 0 {
            out.push(Line::raw(""));
        }
        one(block, width, depth, th, out);
    }
}

fn cells(spans: &[Span]) -> usize {
    spans.iter().map(Span::width).sum()
}

fn dim(text: impl Into<String>, th: Theme) -> Span<'static> {
    Span::styled(text.into(), Style::default().fg(th.dim))
}

fn muted(text: impl Into<String>, th: Theme) -> Span<'static> {
    Span::styled(text.into(), Style::default().fg(th.muted))
}

fn bold(text: impl Into<String>, color: Color) -> Span<'static> {
    Span::styled(text.into(), Style::default().fg(color).add_modifier(Modifier::BOLD))
}

/// `children` drawn behind `prefix`, in the width that leaves.
fn nested(
    children: &[Block],
    prefix: Span<'static>,
    width: usize,
    depth: usize,
    th: Theme,
    out: &mut Vec<Line<'static>>,
) {
    let room = width.saturating_sub(prefix.width()).max(1);
    let mut inner = Vec::new();
    stack(children, room, depth + 1, th, &mut inner);
    for line in inner {
        let mut spans = vec![prefix.clone()];
        spans.extend(line.spans);
        out.push(Line::from(spans));
    }
}

fn one(block: &Block, width: usize, depth: usize, th: Theme, out: &mut Vec<Line<'static>>) {
    if depth > DEEPEST {
        out.push(Line::from(muted("…", th)));
        return;
    }
    if let Some(kind) = desktop_only(block) {
        out.push(Line::from(muted(format!("{kind} — open in the desktop"), th)));
        return;
    }
    match block {
        Block::Section { heading, blocks } => {
            out.push(Line::from(vec![
                Span::styled("▍ ", Style::default().fg(th.accent)),
                bold(one_line(heading), th.text),
            ]));
            nested(blocks, Span::raw("  "), width, depth, th, out);
        }
        Block::Columns { columns } => {
            for (at, column) in columns.iter().enumerate() {
                if at > 0 {
                    out.push(Line::raw(""));
                }
                stack(column, width, depth + 1, th, out);
            }
        }
        Block::Grid { items, .. } => stack(items, width, depth + 1, th, out),
        Block::Tabs { tabs } => tabs_lines(tabs, width, depth, th, out),
        Block::Markdown { text } => markdown(text, width, th, out),
        Block::Fields { fields } => {
            let label = fields
                .iter()
                .map(|f| one_line(&f.label).chars().count())
                .max()
                .unwrap_or(0)
                .min(width / 3)
                .max(1);
            let room = width.saturating_sub(label + 2).max(1);
            for field in fields {
                let name = ellipsize_text(&one_line(&field.label), label);
                let pad = label.saturating_sub(name.chars().count());
                let head = dim(format!("{name}{}  ", " ".repeat(pad)), th);
                for (at, value) in value_lines(&field.value, room, th).into_iter().enumerate() {
                    let lead = if at == 0 { head.clone() } else { Span::raw(" ".repeat(label + 2)) };
                    let mut spans = vec![lead];
                    spans.extend(value);
                    out.push(Line::from(spans));
                }
            }
        }
        Block::Table { columns, rows, sorted_by, total, .. } => {
            table(columns, rows, *sorted_by, *total, width, th, out)
        }
        Block::List { items } => {
            for item in items {
                list_item(item, width, th, out);
            }
        }
        Block::Stats { stats } => stats_lines(stats, width, th, out),
        Block::Code { lang, text } => {
            if let Some(lang) = lang.as_deref().filter(|l| !l.is_empty()) {
                out.push(Line::from(dim(lang.to_string(), th)));
            }
            for line in text.lines() {
                let line = line.replace('\t', "    ");
                out.push(Line::from(vec![
                    dim("│ ", th),
                    Span::styled(
                        ellipsize_text(&line, width.saturating_sub(2)),
                        Style::default().fg(th.text),
                    ),
                ]));
            }
        }
        Block::Callout { tone, text } => {
            let color = tone_color(*tone, th);
            for line in wrap_words(text, width.saturating_sub(2)) {
                out.push(Line::from(vec![
                    Span::styled("▌ ", Style::default().fg(color)),
                    Span::styled(line, Style::default().fg(th.text)),
                ]));
            }
        }
        Block::Card { title, chip, tone, blocks, .. } => {
            let border = if *tone == Tone::Neutral { th.edge } else { tone_color(*tone, th) };
            let edge = Style::default().fg(border);
            if title.is_some() || chip.is_some() {
                let mut head = vec![Span::styled("┌ ", edge)];
                if let Some(title) = title {
                    head.push(bold(one_line(title), th.text));
                }
                if let Some(chip) = chip {
                    head.push(Span::raw(" "));
                    head.push(chip_span(chip, th));
                }
                out.push(Line::from(ellipsize_spans(head, width)));
            }
            nested(blocks, Span::styled("│ ", edge), width, depth, th, out);
            out.push(Line::from(Span::styled("└", edge)));
        }
        Block::Details { summary, chip, blocks, .. } => {
            let mut head = vec![
                Span::styled("▾ ", Style::default().fg(th.accent)),
                bold(one_line(summary), th.text),
            ];
            push_chip(&mut head, chip.as_ref(), th);
            out.push(Line::from(ellipsize_spans(head, width)));
            nested(blocks, Span::raw("  "), width, depth, th, out);
        }
        Block::Row { items, .. } => row(items, width, th, out),
        Block::Divider => out.push(Line::from(Span::styled(
            "─".repeat(width),
            Style::default().fg(th.edge),
        ))),
        Block::Progress { label, value, max, tone, note } => {
            let mut head = vec![Span::styled(one_line(label), Style::default().fg(th.text))];
            let pct = percent(*value, *max);
            let tail = note.as_deref().map(one_line).unwrap_or_default();
            let fixed = head[0].width() + pct.len() + tail.chars().count() + 6;
            let bar = width.saturating_sub(fixed).clamp(6, 24);
            head.push(Span::raw("  "));
            head.push(Span::styled(
                meter(*value, *max, bar),
                Style::default().fg(tone_color(*tone, th)),
            ));
            head.push(muted(format!(" {pct}"), th));
            if !tail.is_empty() {
                head.push(dim(format!("  {tail}"), th));
            }
            out.push(Line::from(ellipsize_spans(head, width)));
        }
        Block::Checklist { items } => {
            for item in items {
                out.push(Line::from(ellipsize_spans(check_item(item, th), width)));
            }
        }
        Block::Log { lines } => {
            for line in lines {
                log_line(line, width, th, out);
            }
        }
        // Named above, so they never get here; the arm keeps a new block
        // from compiling until someone decides how the terminal draws it.
        Block::Chart { .. }
        | Block::Graph { .. }
        | Block::Matrix { .. }
        | Block::Board { .. }
        | Block::Timeline { .. }
        | Block::Tree { .. }
        | Block::Diff { .. }
        | Block::Form { .. }
        | Block::Diagram { .. } => {}
    }
}

fn push_chip(spans: &mut Vec<Span<'static>>, chip: Option<&Chip>, th: Theme) {
    if let Some(chip) = chip {
        spans.push(Span::raw(" "));
        spans.push(chip_span(chip, th));
    }
}

fn tabs_lines(tabs: &[Tab], width: usize, depth: usize, th: Theme, out: &mut Vec<Line<'static>>) {
    for (at, tab) in tabs.iter().enumerate() {
        if at > 0 {
            out.push(Line::raw(""));
        }
        let title = format!("── {} ", one_line(&tab.title));
        let fill = width.saturating_sub(title.chars().count());
        out.push(Line::from(vec![
            bold(title, th.muted),
            Span::styled("─".repeat(fill), Style::default().fg(th.edge)),
        ]));
        stack(&tab.blocks, width, depth + 1, th, out);
    }
}

/// Headings, lists, quotes and fenced code; every other mark is left in the
/// text as written.
fn markdown(text: &str, width: usize, th: Theme, out: &mut Vec<Line<'static>>) {
    let mut fenced = false;
    for raw in text.lines() {
        let line = raw.trim_end().replace('\t', "    ");
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            out.push(Line::from(vec![
                dim("│ ", th),
                Span::styled(
                    ellipsize_text(&line, width.saturating_sub(2)),
                    Style::default().fg(th.text),
                ),
            ]));
            continue;
        }
        if trimmed.is_empty() {
            out.push(Line::raw(""));
            continue;
        }
        let level = trimmed.chars().take_while(|c| *c == '#').count();
        if (1..=6).contains(&level) && trimmed[level..].starts_with(' ') {
            let color = if level == 1 { th.accent } else { th.text };
            for part in wrap_words(trimmed[level..].trim(), width) {
                out.push(Line::from(bold(part, color)));
            }
            continue;
        }
        let indent = line.len() - trimmed.len();
        if let Some(rest) = ["- ", "* ", "+ "].iter().find_map(|m| trimmed.strip_prefix(m)) {
            let lead = " ".repeat(indent.min(width / 2));
            hanging(&format!("{lead}• "), rest, Style::default().fg(th.text), width, th, out);
        } else if let Some(rest) = trimmed.strip_prefix("> ") {
            hanging("│ ", rest, Style::default().fg(th.muted), width, th, out);
        } else if let Some((marker, rest)) = numbered(trimmed) {
            hanging(&marker, rest, Style::default().fg(th.text), width, th, out);
        } else {
            for part in wrap_words(trimmed, width) {
                out.push(Line::from(Span::styled(part, Style::default().fg(th.text))));
            }
        }
    }
}

/// `3. text` as its marker and the text after it.
fn numbered(line: &str) -> Option<(String, &str)> {
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    let rest = line[digits..].strip_prefix(". ")?;
    (digits > 0).then(|| (format!("{}. ", &line[..digits]), rest))
}

/// `text` wrapped behind `marker`, its later lines indented under the first.
fn hanging(
    marker: &str,
    text: &str,
    style: Style,
    width: usize,
    th: Theme,
    out: &mut Vec<Line<'static>>,
) {
    let lead = marker.chars().count();
    let marker_style = Style::default().fg(th.dim);
    for (at, part) in wrap_words(text, width.saturating_sub(lead).max(1)).into_iter().enumerate() {
        // A quote's bar runs down its lines; a bullet or number does not.
        let head = if at == 0 || marker.starts_with('│') {
            Span::styled(marker.to_string(), marker_style)
        } else {
            Span::raw(" ".repeat(lead))
        };
        out.push(Line::from(vec![head, Span::styled(part, style)]));
    }
}

fn table(
    columns: &[Column],
    rows: &[Vec<Value>],
    sorted_by: Option<usize>,
    total: Option<u64>,
    width: usize,
    th: Theme,
    out: &mut Vec<Line<'static>>,
) {
    let titles: Vec<String> = columns
        .iter()
        .enumerate()
        .map(|(at, c)| {
            let sorted = if sorted_by == Some(at) { " ▾" } else { "" };
            format!("{}{sorted}", one_line(&c.title))
        })
        .collect();
    let body: Vec<Vec<Vec<Span<'static>>>> = rows
        .iter()
        .map(|row| {
            (0..columns.len())
                .map(|at| row.get(at).map_or_else(Vec::new, |v| value_spans(v, th)))
                .collect()
        })
        .collect();
    let mut widths: Vec<usize> = (0..columns.len())
        .map(|at| {
            let cell = body.iter().map(|row| cells(&row[at])).max().unwrap_or(0);
            cell.max(titles[at].chars().count())
        })
        .collect();
    let mut shown: Vec<usize> = (0..columns.len()).collect();
    let span = |shown: &[usize], widths: &[usize]| {
        shown.iter().map(|&at| widths[at]).sum::<usize>() + 2 * shown.len().saturating_sub(1)
    };
    // The columns that matter stay: an optional one goes, last first, before
    // any column is squeezed.
    while span(&shown, &widths) > width {
        let Some(drop) = shown.iter().rposition(|&at| columns[at].optional) else {
            break;
        };
        shown.remove(drop);
    }
    while span(&shown, &widths) > width {
        let Some(&widest) = shown.iter().max_by_key(|&&at| widths[at]) else {
            break;
        };
        if widths[widest] <= 3 {
            break;
        }
        widths[widest] -= 1;
    }
    let pad = |mut spans: Vec<Span<'static>>, room: usize, right: bool| {
        let mut spans = ellipsize_spans(std::mem::take(&mut spans), room);
        let fill = " ".repeat(room.saturating_sub(cells(&spans)));
        if right {
            spans.insert(0, Span::raw(fill));
        } else {
            spans.push(Span::raw(fill));
        }
        spans
    };
    let mut head = Vec::new();
    for (place, &at) in shown.iter().enumerate() {
        if place > 0 {
            head.push(Span::raw("  "));
        }
        let title = Span::styled(
            titles[at].clone(),
            Style::default().fg(th.muted).add_modifier(Modifier::BOLD),
        );
        head.extend(pad(vec![title], widths[at], columns[at].numeric));
    }
    out.push(Line::from(head));
    out.push(Line::from(Span::styled(
        "─".repeat(span(&shown, &widths).min(width)),
        Style::default().fg(th.edge),
    )));
    for row in body {
        let mut spans = Vec::new();
        for (place, &at) in shown.iter().enumerate() {
            if place > 0 {
                spans.push(Span::raw("  "));
            }
            spans.extend(pad(row[at].clone(), widths[at], columns[at].numeric));
        }
        out.push(Line::from(spans));
    }
    if rows.is_empty() {
        out.push(Line::from(dim("no rows", th)));
    }
    if let Some(total) = total.filter(|t| *t > rows.len() as u64) {
        out.push(Line::from(dim(format!("{} of {total} rows", rows.len()), th)));
    }
}

fn list_item(item: &Item, width: usize, th: Theme, out: &mut Vec<Line<'static>>) {
    let mut head: Vec<Span<'static>> = Vec::new();
    if let Some(who) = &item.who {
        head.push(bold(one_line(who), th.text));
    }
    if let Some(at) = &item.at {
        let sep = if head.is_empty() { "" } else { " · " };
        head.push(dim(format!("{sep}{}", date_time_text(at)), th));
    }
    if let Some(title) = &item.title {
        let sep = if head.is_empty() { "" } else { " · " };
        head.push(Span::styled(format!("{sep}{}", one_line(title)), Style::default().fg(th.text)));
    }
    push_chip(&mut head, item.chip.as_ref(), th);
    let bullet = Span::styled("• ", Style::default().fg(th.accent));
    let mut body = Vec::new();
    if let Some(text) = &item.body {
        markdown(text, width.saturating_sub(2).max(1), th, &mut body);
    }
    if head.is_empty() {
        // Nothing to head it: the bullet leads the body itself.
        for (at, line) in body.into_iter().enumerate() {
            let lead = if at == 0 { bullet.clone() } else { Span::raw("  ") };
            let mut spans = vec![lead];
            spans.extend(line.spans);
            out.push(Line::from(spans));
        }
        return;
    }
    let mut spans = vec![bullet];
    spans.extend(head);
    out.push(Line::from(ellipsize_spans(spans, width)));
    for line in body {
        let mut spans = vec![Span::raw("  ")];
        spans.extend(line.spans);
        out.push(Line::from(spans));
    }
}

fn stats_lines(stats: &[Stat], width: usize, th: Theme, out: &mut Vec<Line<'static>>) {
    let label = stats
        .iter()
        .map(|s| one_line(&s.label).chars().count())
        .max()
        .unwrap_or(0)
        .min(width / 3)
        .max(1);
    for stat in stats {
        let name = ellipsize_text(&one_line(&stat.label), label);
        let pad = label.saturating_sub(name.chars().count());
        let mut spans = vec![
            dim(format!("{name}{}  ", " ".repeat(pad)), th),
            bold(one_line(&stat.value), tone_color(stat.tone, th)),
        ];
        if let Some(delta) = &stat.delta {
            spans.push(muted(format!("  {}", one_line(delta)), th));
        }
        if !stat.trend.is_empty() {
            spans.push(dim(format!("  {}", spark(&stat.trend)), th));
        }
        out.push(Line::from(ellipsize_spans(spans, width)));
    }
}

/// Values and actions on lines, wrapping where they run out of room. An
/// action is shown as the label of a button the desktop has, not pressable.
fn row(items: &[RowItem], width: usize, th: Theme, out: &mut Vec<Line<'static>>) {
    let mut line: Vec<Span<'static>> = Vec::new();
    for item in items {
        let spans = match item {
            RowItem::Value { value } => value_spans(value, th),
            RowItem::Action { action } => vec![dim(format!("[{}]", one_line(&action.label)), th)],
        };
        let sep = if line.is_empty() { 0 } else { 3 };
        if !line.is_empty() && cells(&line) + sep + cells(&spans) > width {
            out.push(Line::from(std::mem::take(&mut line)));
        }
        if !line.is_empty() {
            line.push(dim(" · ", th));
        }
        line.extend(spans);
    }
    if !line.is_empty() {
        out.push(Line::from(ellipsize_spans(line, width)));
    }
}

fn check_item(item: &CheckItem, th: Theme) -> Vec<Span<'static>> {
    let (mark, color) = if item.done { ("[x] ", th.ok) } else { ("[ ] ", th.muted) };
    let mut spans = vec![
        Span::styled(mark, Style::default().fg(color)),
        Span::styled(
            one_line(&item.label),
            Style::default().fg(if item.done { th.muted } else { th.text }),
        ),
    ];
    if let Some(note) = &item.note {
        spans.push(dim(format!(" — {}", one_line(note)), th));
    }
    spans
}

fn log_line(line: &LogLine, width: usize, th: Theme, out: &mut Vec<Line<'static>>) {
    let (tag, color) = match line.level {
        LogLevel::Debug => ("DEBUG", th.dim),
        LogLevel::Info => ("INFO ", th.muted),
        LogLevel::Warn => ("WARN ", th.warn),
        LogLevel::Error => ("ERROR", th.err),
    };
    let at = line.at.as_deref().map(date_time_text).unwrap_or_default();
    let mut lead = vec![];
    // A timestamp that would take the text's room is the first thing to go.
    if !at.is_empty() && at.chars().count() + 8 < width / 2 {
        lead.push(dim(format!("{at} "), th));
    }
    lead.push(Span::styled(format!("{tag} "), Style::default().fg(color)));
    let room = width.saturating_sub(cells(&lead)).max(1);
    for (place, part) in wrap_words(&line.text, room).into_iter().enumerate() {
        let mut spans = if place == 0 {
            lead.clone()
        } else {
            vec![Span::raw(" ".repeat(cells(&lead)))]
        };
        spans.push(Span::styled(part, Style::default().fg(th.text)));
        out.push(Line::from(spans));
    }
}
