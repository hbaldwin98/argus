//! The colours of a tree-sitter capture, from the token space: the kind each capture's first word
//! stands for is `argus-protocol`'s table, the one the daemon's read view colours by, and this is
//! where a kind becomes a colour of the active theme (the page's `sx-*` rules). Also the one place
//! the kit's capture names and Argus's are reconciled, since the kit's highlighter resolves only
//! the names it lists.

use argus_protocol::review::captures::CAPTURES;
use argus_protocol::HighlightKind;
use gpui_kit::component::highlighter::SyntaxColors;
use gpui_kit::{Hsla, Rgba};
use serde_json::{json, Map, Value};

use super::Tokens;

/// The theme's colour for a kind of token, and whether it is set in italics.
pub fn colour(kind: HighlightKind, t: &Tokens) -> (Hsla, bool) {
    match kind {
        HighlightKind::Keyword => (t.mauve, false),
        HighlightKind::Str => (t.green, false),
        HighlightKind::Comment => (t.fg_faint, true),
        HighlightKind::Number | HighlightKind::Constant => (t.peach, false),
        HighlightKind::Type => (t.yellow, false),
        HighlightKind::Function => (t.blue, false),
        HighlightKind::Property => (t.teal, false),
        HighlightKind::Operator | HighlightKind::Punctuation => (t.fg_muted, false),
    }
}

fn hex(colour: Hsla) -> String {
    let rgba = Rgba::from(colour);
    let byte = |c: f32| (c * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}{:02x}", byte(rgba.r), byte(rgba.g), byte(rgba.b), byte(rgba.a))
}

/// The kit's syntax colours for `t`: each capture word the table names, and nothing else, so a
/// plain variable and a word the table lacks stay the text's colour (EDITOR-COLOUR-1). A dotted
/// capture the kit resolves by its first word takes its colour.
pub fn colours(t: &Tokens) -> SyntaxColors {
    let styles: Map<String, Value> = CAPTURES
        .iter()
        .map(|(name, kind)| {
            let (colour, italic) = colour(*kind, t);
            let mut style = json!({ "color": hex(colour) });
            if italic {
                style["font_style"] = json!("italic");
            }
            ((*name).to_owned(), style)
        })
        .collect();
    // Names the kit has no field for are ignored by the reading; `query` renames them instead.
    serde_json::from_value(Value::Object(styles)).expect("the capture table names styles the kit can read")
}

/// The kit's name for a capture word the kit lists no style for, which carries the same kind.
fn kit_name(word: &str) -> Option<&'static str> {
    match word {
        "escape" => Some("string.escape"),
        "module" | "namespace" => Some("type"),
        _ => None,
    }
}

/// A grammar's highlight query with the captures the kit would leave uncoloured renamed to the
/// ones it colours the same: `@namespace` and `@module` as `@type`, `@escape` as `@string.escape`.
/// Only a whole first word is renamed, so `@module.builtin` follows `@module` and `@modules` is
/// left alone.
pub fn query(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut rest = source;
    while let Some(at) = rest.find('@') {
        out.push_str(&rest[..=at]);
        rest = &rest[at + 1..];
        let end = rest.find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '$'))).unwrap_or(rest.len());
        let (capture, after) = rest.split_at(end);
        let word = capture.split('.').next().unwrap_or(capture);
        match kit_name(word) {
            Some(name) => {
                out.push_str(name);
                out.push_str(&capture[word.len()..]);
            }
            None => out.push_str(capture),
        }
        rest = after;
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests;
