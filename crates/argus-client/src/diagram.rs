//! A sequence diagram open in the floating overlay: Mermaid source rendered
//! to Unicode box-drawing via `mermaid-text`.

use argus_protocol::SequenceDiagram;

/// One diagram, rendered for the current terminal width.
pub struct DiagramView {
    pub title: String,
    /// Lines to draw, or a single error line when rendering failed.
    pub lines: Vec<String>,
    pub scroll: usize,
}

impl DiagramView {
    pub fn open(diagram: &SequenceDiagram, width: usize) -> DiagramView {
        let max_width = width.max(20);
        let lines = match mermaid_text::render_with_width(&diagram.body, Some(max_width)) {
            Ok(text) => text.lines().map(str::to_string).collect(),
            Err(err) => vec![format!("could not render diagram: {err}")],
        };
        DiagramView {
            title: diagram.title.clone(),
            scroll: 0,
            lines,
        }
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
}
