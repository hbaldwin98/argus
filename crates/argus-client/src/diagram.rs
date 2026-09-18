//! A sequence diagram open in the floating overlay: Mermaid source rendered
//! to Unicode box-drawing via `mermaid-text`.

use argus_protocol::SequenceDiagram;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// One diagram, rendered for the current terminal width.
pub struct DiagramView {
    pub title: String,
    /// Lines to draw, or a single error line when rendering failed.
    pub lines: Vec<String>,
    pub scroll: usize,
    source: String,
    rendered_width: usize,
    horizontal_scroll: usize,
}

impl DiagramView {
    pub fn open(diagram: &SequenceDiagram, width: usize) -> DiagramView {
        let rendered_width = width.max(1);
        DiagramView {
            title: diagram.title.clone(),
            scroll: 0,
            lines: render_lines(&diagram.body, rendered_width),
            source: diagram.body.clone(),
            rendered_width,
            horizontal_scroll: 0,
        }
    }

    /// Re-render after the overlay changes size. Sequence diagrams in
    /// `mermaid-text` have a fixed natural layout, so the width-aware pass
    /// belongs here rather than in the dependency's graph renderer.
    pub fn resize(&mut self, width: usize) {
        let width = width.max(1);
        if self.rendered_width == width {
            return;
        }
        self.rendered_width = width;
        self.lines = render_lines(&self.source, width);
        self.horizontal_scroll = self
            .horizontal_scroll
            .min(self.max_horizontal_scroll(width));
    }

    pub fn follow_cursor(&mut self, visible: usize) {
        if visible == 0 {
            return;
        }
        let max_scroll = self.lines.len().saturating_sub(visible);
        if self.scroll > max_scroll {
            self.scroll = max_scroll;
        }
    }

    pub fn scroll_by(&mut self, delta: i32, visible: usize) {
        let max_scroll = self.lines.len().saturating_sub(visible) as i32;
        self.scroll = (self.scroll as i32 + delta).clamp(0, max_scroll) as usize;
    }

    pub fn follow_horizontal(&mut self, visible: usize) {
        self.horizontal_scroll = self
            .horizontal_scroll
            .min(self.max_horizontal_scroll(visible));
    }

    pub fn scroll_horizontal_by(&mut self, delta: i32, visible: usize) {
        let max_scroll = self.max_horizontal_scroll(visible) as i32;
        self.horizontal_scroll =
            (self.horizontal_scroll as i32 + delta).clamp(0, max_scroll) as usize;
    }

    pub fn line_window(&self, line: &str, visible: usize) -> String {
        window(line, self.horizontal_scroll, visible)
    }

    fn max_horizontal_scroll(&self, visible: usize) -> usize {
        max_line_width(&self.lines).saturating_sub(visible.max(1))
    }
}

/// Render sequence source into a clean, terminal-sized text grid.
///
/// `mermaid-text` intentionally fills control-flow frames with shade glyphs,
/// which makes an otherwise readable `alt` block look like noise in a TUI.
/// Its sequence renderer also has no compaction pass, so first remove that
/// decorative fill and then collapse columns containing only whitespace and
/// horizontal strokes. If a diagram still cannot fit, the view keeps its full
/// lines and lets the overlay expose the remainder with horizontal scrolling.
fn render_lines(source: &str, width: usize) -> Vec<String> {
    match mermaid_text::render(source) {
        Ok(text) => compact_lines(&text, width),
        Err(err) => vec![format!("could not render diagram: {err}")],
    }
}

fn compact_lines(text: &str, width: usize) -> Vec<String> {
    let mut grid: Vec<Vec<RenderCell>> = text
        .lines()
        .map(|line| cells(&clean_fill(line)))
        .collect();
    let grid_width = grid.iter().map(Vec::len).max().unwrap_or(0);
    for row in &mut grid {
        row.resize(grid_width, RenderCell::Blank);
    }

    while grid_width_of(&grid) > width.max(1) {
        let current_width = grid_width_of(&grid);
        let Some(column) = (0..current_width)
            .filter(|&column| removable_column(&grid, column))
            .max_by_key(|&column| blank_cells(&grid, column))
        else {
            break;
        };
        for row in &mut grid {
            row.remove(column);
        }
    }

    grid.into_iter().map(row_string).collect()
}

#[derive(Clone, Copy)]
enum RenderCell {
    Blank,
    Glyph(char),
    Continuation,
}

fn clean_fill(line: &str) -> String {
    line.chars()
        .map(|ch| match ch {
            // Control-flow frame fills are decorative, and their dense visual
            // texture is especially noisy against a terminal's own background.
            '\u{2591}' | '\u{2592}' | '\u{2593}' => ' ',
            _ => ch,
        })
        .collect()
}

fn cells(line: &str) -> Vec<RenderCell> {
    let mut cells = Vec::new();
    for ch in line.chars() {
        cells.push(RenderCell::Glyph(ch));
        for _ in 1..UnicodeWidthChar::width(ch).unwrap_or(1) {
            cells.push(RenderCell::Continuation);
        }
    }
    cells
}

fn grid_width_of(grid: &[Vec<RenderCell>]) -> usize {
    grid.first().map_or(0, Vec::len)
}

fn removable_column(grid: &[Vec<RenderCell>], column: usize) -> bool {
    grid.iter().all(|row| match row[column] {
        RenderCell::Blank => true,
        RenderCell::Glyph(ch) => is_horizontal_stroke(ch),
        RenderCell::Continuation => false,
    })
}

fn blank_cells(grid: &[Vec<RenderCell>], column: usize) -> usize {
    grid.iter()
        .filter(|row| matches!(row[column], RenderCell::Blank))
        .count()
}

fn is_horizontal_stroke(ch: char) -> bool {
    matches!(
        ch,
        '─' | '━'
            | '┄'
            | '┈'
            | '╌'
            | '╍'
            | '═'
            | '╴'
            | '╶'
            | '╸'
            | '╺'
    )
}

fn row_string(row: Vec<RenderCell>) -> String {
    let mut line = String::new();
    for cell in row {
        if let RenderCell::Glyph(ch) = cell {
            line.push(ch);
        } else if matches!(cell, RenderCell::Blank) {
            line.push(' ');
        }
    }
    line.trim_end().to_string()
}

fn max_line_width(lines: &[String]) -> usize {
    lines
        .iter()
        .map(|line| UnicodeWidthStr::width(line.as_str()))
        .max()
        .unwrap_or(0)
}

fn window(line: &str, start: usize, width: usize) -> String {
    let end = start.saturating_add(width.max(1));
    let mut offset = 0;
    let mut out = String::new();
    for ch in line.chars() {
        let char_width = UnicodeWidthChar::width(ch).unwrap_or(1);
        let next = offset + char_width;
        if next <= start {
            offset = next;
            continue;
        }
        if offset < start {
            out.push(' ');
        } else if next <= end {
            out.push(ch);
        } else {
            break;
        }
        offset = next;
    }
    out.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_protocol::SequenceDiagram;

    #[test]
    fn mermaid_sequence_source_renders_to_multiple_lines() {
        let diagram = SequenceDiagram {
            id: 1,
            feature: "x".into(),
            title: "t".into(),
            body: "sequenceDiagram\n    A->>B: ping".into(),
            at: 0,
            session: None,
        };
        let view = DiagramView::open(&diagram, 60);
        assert!(view.lines.len() > 1);
        assert!(view.lines.iter().any(|l| l.contains('A') || l.contains('B')));
    }

    #[test]
    fn an_alt_diagram_drops_fill_noise_and_supports_horizontal_panning() {
        let diagram = SequenceDiagram {
            id: 2,
            feature: "x".into(),
            title: "conditional".into(),
            body: "sequenceDiagram
    participant Client
    participant Daemon
    participant Hook
    Client->>Daemon: request
    alt accepted
        Daemon-->>Client: accepted
        Daemon->>Hook: notify
    else rejected
        Daemon-->>Client: rejected
    end"
                .into(),
            at: 0,
            session: None,
        };
        let view = DiagramView::open(&diagram, 40);
        assert!(view.lines.iter().all(|line| {
            !line.contains('░') && !line.contains('▒') && !line.contains('▓')
        }));
        assert!(max_line_width(&view.lines) > 40);

        let left = view.line_window(&view.lines[0], 40);
        let mut view = view;
        view.scroll_horizontal_by(10, 40);
        let right = view.line_window(&view.lines[0], 40);
        assert!(UnicodeWidthStr::width(left.as_str()) <= 40);
        assert!(UnicodeWidthStr::width(right.as_str()) <= 40);
        assert_ne!(left, right, "a wide diagram must be pannable");
    }

    #[test]
    fn resizing_reflows_the_diagram_to_the_overlay_width() {
        let diagram = SequenceDiagram {
            id: 3,
            feature: "x".into(),
            title: "t".into(),
            body: "sequenceDiagram
    participant Client
    participant Daemon
    Client->>Daemon: request".into(),
            at: 0,
            session: None,
        };
        let mut view = DiagramView::open(&diagram, 20);
        view.resize(80);
        assert!(max_line_width(&view.lines) <= 80);
    }
}
