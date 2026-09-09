//! Markdown, styled where it stands.
//!
//! Notes, feature briefs, and task briefs are prose that people write in markdown and
//! then read on this screen, and they used to arrive here as one
//! undifferentiated colour: a heading looked like a paragraph, and the
//! only thing picked out of a brief was its checkboxes. A document whose
//! structure is invisible is one nobody skims.
//!
//! **Nothing is hidden.** A `**` stays on screen, dimmed, rather than
//! being consumed by the span it opens. This is an editor as much as a
//! reader — the same lines are typed into in insert mode — and a renderer
//! that eats characters puts the caret somewhere other than where the
//! cursor says it is. Dimming the markers instead gives the structure its
//! weight while every column still means what it says.

use ratatui::style::{Modifier, Style};
use ratatui::text::Span;

use crate::theme::Theme;

/// One line of prose, marked up. `base` is the line's own style: the
/// selection bar it sits under, and — where it names one — the colour its
/// plain text takes. A brief introducing a board is drawn quieter than a
/// note being edited, and it is the same markup either way, so the caller
/// says how loud and this says what the shapes are.
pub(super) fn prose_spans(text: &str, base: Style, th: Theme) -> Vec<Span<'static>> {
    let plain = base.fg.unwrap_or(th.text);
    if let Some(spans) = block_line(text, base, th, plain) {
        return spans;
    }
    inline_spans(text, base, th, plain)
}

/// The line-level shapes: a heading, a quote, a rule. Each takes the whole
/// line, so they are decided before any inline scan.
fn block_line(
    text: &str,
    base: Style,
    th: Theme,
    plain: ratatui::style::Color,
) -> Option<Vec<Span<'static>>> {
    let trimmed = text.trim_start();
    let indent = &text[..text.len() - trimmed.len()];

    // A rule is structure with no content, so it is drawn as chrome.
    if trimmed.len() >= 3
        && (trimmed.chars().all(|c| c == '-')
            || trimmed.chars().all(|c| c == '*')
            || trimmed.chars().all(|c| c == '_'))
    {
        return Some(vec![Span::styled(text.to_string(), base.fg(th.dim))]);
    }

    if let Some(rest) = trimmed.strip_prefix('>') {
        return Some(vec![
            Span::styled(format!("{indent}>"), base.fg(th.dim)),
            Span::styled(
                rest.to_string(),
                base.fg(th.muted).add_modifier(Modifier::ITALIC),
            ),
        ]);
    }

    let hashes = trimmed.chars().take_while(|c| *c == '#').count();
    if (1..=6).contains(&hashes) && trimmed.chars().nth(hashes) == Some(' ') {
        let (marker, rest) = trimmed.split_at(hashes);
        // Deeper headings are quieter: a document whose every level shouts
        // at the same volume has no levels.
        let colour = if hashes <= 2 { th.accent } else { plain };
        return Some(vec![
            Span::styled(format!("{indent}{marker}"), base.fg(th.dim)),
            Span::styled(
                rest.to_string(),
                base.fg(colour).add_modifier(Modifier::BOLD),
            ),
        ]);
    }

    None
}

/// The inline shapes, scanned left to right: code, strong, emphasis.
///
/// Deliberately not a markdown parser. These are the four spellings that
/// actually appear in the notes people write here, and a real parser would
/// bring a document model this screen has no use for — the line is the
/// unit, because the line is what the cursor moves through.
fn inline_spans(
    text: &str,
    base: Style,
    th: Theme,
    plain: ratatui::style::Color,
) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut buf = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    let flush = |text: &mut String, spans: &mut Vec<Span<'static>>| {
        if !text.is_empty() {
            spans.push(Span::styled(std::mem::take(text), base.fg(plain)));
        }
    };

    while i < chars.len() {
        let rest = &chars[i..];
        let delimiter = if rest.starts_with(&['`']) {
            Some(("`", 1))
        } else if rest.starts_with(&['*', '*']) {
            Some(("**", 2))
        } else if rest.starts_with(&['*']) {
            Some(("*", 1))
        } else if rest.starts_with(&['_']) {
            Some(("_", 1))
        } else {
            None
        };

        let Some((mark, len)) = delimiter else {
            buf.push(chars[i]);
            i += 1;
            continue;
        };

        let Some(close) = closing(&chars[i + len..], mark) else {
            // An unmatched marker is a literal asterisk, which is what the
            // person typing one halfway through a word meant by it.
            buf.push(chars[i]);
            i += 1;
            continue;
        };

        let body: String = chars[i + len..i + len + close].iter().collect();
        let style = match mark {
            "`" => base.fg(th.syntax.string),
            "**" => base.fg(plain).add_modifier(Modifier::BOLD),
            _ => base.fg(plain).add_modifier(Modifier::ITALIC),
        };
        flush(&mut buf, &mut spans);
        spans.push(Span::styled(mark.to_string(), base.fg(th.dim)));
        spans.push(Span::styled(body, style));
        spans.push(Span::styled(mark.to_string(), base.fg(th.dim)));
        i += len + close + len;
    }

    flush(&mut buf, &mut spans);
    if spans.is_empty() {
        spans.push(Span::styled(String::new(), base.fg(plain)));
    }
    spans
}

/// How far to the matching close of `mark`, in characters, or `None` when
/// the line does not close it. An empty span (`****`) does not count: it
/// is four literal asterisks, not emphasis around nothing.
fn closing(rest: &[char], mark: &str) -> Option<usize> {
    let mark: Vec<char> = mark.chars().collect();
    (1..=rest.len().saturating_sub(mark.len())).find(|i| rest[*i..].starts_with(&mark[..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn th() -> Theme {
        Theme::mocha()
    }

    /// The spans' text, which must always add back up to the input: this
    /// screen is typed into, so a renderer that drops a character puts the
    /// caret somewhere other than where the cursor says it is.
    fn text_of(spans: &[Span<'static>]) -> String {
        spans.iter().map(|s| s.content.as_ref()).collect()
    }

    fn spans(line: &str) -> Vec<Span<'static>> {
        prose_spans(line, Style::default(), th())
    }

    #[test]
    fn every_character_survives_being_marked_up() {
        for line in [
            "# a heading",
            "plain prose",
            "some **strong** words",
            "an *emphasis* here",
            "a `code` span",
            "> a quotation",
            "---",
            "mixed **strong** and `code` and *emphasis*",
            "an unmatched ** marker",
            "a_snake_case_name",
            "",
        ] {
            assert_eq!(text_of(&spans(line)), line, "for {line:?}");
        }
    }

    #[test]
    fn a_heading_is_lifted_and_its_hashes_recede() {
        let out = spans("## the shape of it");
        assert_eq!(out[0].content, "##");
        assert_eq!(out[0].style.fg, Some(th().dim));
        assert_eq!(out[1].style.fg, Some(th().accent));
        assert!(out[1].style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn deeper_headings_are_quieter_than_the_top_two() {
        let shallow = spans("# top");
        let deep = spans("#### deep");
        assert_eq!(shallow[1].style.fg, Some(th().accent));
        assert_eq!(deep[1].style.fg, Some(th().text));
        assert!(deep[1].style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn six_hashes_is_a_heading_and_seven_is_prose() {
        assert_eq!(spans("###### still a heading")[0].content, "######");
        let seven = spans("####### not one");
        assert_eq!(seven.len(), 1, "a run past six is just text: {seven:?}");
    }

    #[test]
    fn a_hash_with_no_space_after_it_is_not_a_heading() {
        // `#4` is an issue number, and the notes here are full of them.
        let out = spans("#4 is not a title");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].style.fg, Some(th().text));
    }

    #[test]
    fn the_inline_markers_get_their_own_weight() {
        let strong = spans("a **word** here");
        let marked: Vec<&str> = strong.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(marked, vec!["a ", "**", "word", "**", " here"]);
        assert!(strong[2].style.add_modifier.contains(Modifier::BOLD));
        // The markers stay on screen but recede, so the emphasis reads
        // without the line lying about how many columns it has.
        assert_eq!(strong[1].style.fg, Some(th().dim));

        let code = spans("a `call()` here");
        assert_eq!(code[2].content, "call()");
        assert_eq!(code[2].style.fg, Some(th().syntax.string));

        let em = spans("an *aside* here");
        assert!(em[2].style.add_modifier.contains(Modifier::ITALIC));
    }

    #[test]
    fn an_unclosed_marker_is_left_as_typed() {
        let out = spans("half a **thought");
        assert_eq!(out.len(), 1, "{out:?}");
        assert_eq!(out[0].style.fg, Some(th().text));
        assert!(!out[0].style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn an_empty_span_is_four_literal_asterisks() {
        let out = spans("****");
        assert_eq!(text_of(&out), "****");
        assert!(out
            .iter()
            .all(|s| !s.style.add_modifier.contains(Modifier::BOLD)));
    }

    #[test]
    fn a_quotation_recedes_into_italic() {
        let out = spans("> somebody else said this");
        assert_eq!(out[0].content, ">");
        assert_eq!(out[1].style.fg, Some(th().muted));
        assert!(out[1].style.add_modifier.contains(Modifier::ITALIC));
    }

    #[test]
    fn a_rule_is_drawn_as_chrome() {
        for rule in ["---", "***", "___", "-----"] {
            let out = spans(rule);
            assert_eq!(out.len(), 1, "for {rule:?}");
            assert_eq!(out[0].style.fg, Some(th().dim), "for {rule:?}");
        }
        // Two of them is not a rule, and `--` opens a command-line flag.
        assert_ne!(spans("--")[0].style.fg, Some(th().dim));
    }

    #[test]
    fn an_indented_heading_keeps_its_indent() {
        let out = spans("    ## nested");
        assert_eq!(text_of(&out), "    ## nested");
        assert_eq!(out[0].content, "    ##");
    }

    #[test]
    fn the_line_style_underneath_is_kept() {
        // The cursor's row carries a selection bar, and marking the line
        // up must not punch holes in it.
        let bar = Style::default().bg(th().sel_bg);
        let out = prose_spans("a **strong** word", bar, th());
        assert!(
            out.iter().all(|s| s.style.bg == Some(th().sel_bg)),
            "every span keeps the bar: {out:?}"
        );
    }

    #[test]
    fn multibyte_prose_is_split_on_characters_not_bytes() {
        let line = "a **wörd** — with an em dash";
        let out = spans(line);
        assert_eq!(text_of(&out), line);
        assert_eq!(out[2].content, "wörd");
    }
}
