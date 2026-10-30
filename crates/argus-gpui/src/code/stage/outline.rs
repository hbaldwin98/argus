//! The right column of the Code stage, shown while the outline is on and a file is read: its
//! header with the button that puts it away, what the file defines, and the changes in it
//! (`outline`, `code/stage.ts`), and the sash that sizes it. Drawn from the kit's panel head, row,
//! section head and kind squares, as the board has it (`project/Code.dc.html`).
//!
//! Which definition the cursor is in, which changed and what each change says is core's, sent in the
//! page (`CodeOutlineRow`, `CodeChangeRow`); a row sends the line it stands for. A method is drawn
//! under the type before it (`nesting`), as the grammar says it is defined inside one.

use argus_desktop_core::code::outline::{empty, CHANGES, CLOSE, HEADING};
use argus_desktop_core::code::stage::{CodeChangeRow, CodeClick, CodeGesture, CodeOutlineRow, CodePage};
use gpui_kit::component::v_flex;
use gpui_kit::{div, prelude::*, relative, AnyElement, Context, SharedString, TestSupportExt as _};

use super::CodeStage;
use crate::kit::icon_button::{icon_button, Node};
use crate::kit::icons::Glyph;
use crate::kit::kind::{bar, square, Kind};
use crate::kit::panel_head::panel_head;
use crate::kit::row::{end_text, nest, row, Slots};
use crate::kit::section;
use crate::kit::tip::tip;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// One top-level entry of the outline: a definition and the methods drawn under it.
struct Entry {
    at: usize,
    methods: Vec<usize>,
}

/// The definitions in the order they come, a method placed under the nearest definition before it
/// that is not itself a method. A method with nothing before it stands on its own.
fn nesting(symbols: &[CodeOutlineRow]) -> Vec<Entry> {
    let mut out: Vec<Entry> = Vec::new();
    for (at, symbol) in symbols.iter().enumerate() {
        match out.last_mut() {
            Some(entry) if symbol.method => entry.methods.push(at),
            _ => out.push(Entry { at, methods: Vec::new() }),
        }
    }
    out
}

/// What a definition is drawn as. The grammar tags a struct and its `impl` alike as a type, so a
/// type named again after an earlier one is the `impl` of it.
fn kind_of(symbols: &[CodeOutlineRow], at: usize) -> (Kind, String) {
    let symbol = &symbols[at];
    let repeated = symbol.kind == "type" && symbols[..at].iter().any(|before| before.kind == "type" && before.name == symbol.name);
    let word = if repeated { "impl" } else { symbol.kind };
    let kind = Kind::of(word);
    (kind, kind.letter(word))
}

/// `.row` for a change: the 3px bar in the icon slot, `Line 5` or `Lines 14–22`, and its size at
/// the end.
fn change_label(change: &CodeChangeRow) -> String {
    if change.lines <= 1 {
        format!("Line {}", change.line)
    } else {
        format!("Lines {}\u{2013}{}", change.line, change.line + change.lines - 1)
    }
}

impl CodeStage {
    /// `.code-outline`: `width` wide and at most `at_most` of the stage, a hairline on its left that
    /// is also its sash.
    pub(super) fn outline(&self, page: &CodePage, width: f32, at_most: f32, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let entries = nesting(&page.symbols);
        let symbol_row = |at: usize, cx: &mut Context<Self>| -> Node {
            let symbol = &page.symbols[at];
            let (kind, letter) = kind_of(&page.symbols, at);
            let line = symbol.line;
            let slots = Slots {
                icon: Some(square(letter, kind, tokens).into_any_element()),
                end: symbol.changed.then(|| div().id(("code-symbol-changed", at)).test_support().aria_label("changed").size(rem(tokens.dot_sm)).rounded_full().bg(tokens.changed).into_any_element()),
                on: symbol.on,
                ..Slots::new(symbol.name.clone())
            };
            row(("code-symbol", at), slots, tokens).on_click(cx.listener(move |this, _, _, cx| this.send(CodeGesture::Click { target: CodeClick::Go { line } }, cx)))
        };
        let mut symbols: Vec<AnyElement> = Vec::new();
        for entry in &entries {
            symbols.push(symbol_row(entry.at, cx).into_any_element());
            if !entry.methods.is_empty() {
                let methods = entry.methods.iter().map(|&at| symbol_row(at, cx).into_any_element()).collect();
                symbols.push(nest(("code-symbol-methods", entry.at), methods, tokens).into_any_element());
            }
        }
        let changes = page.changes_here.iter().enumerate().map(|(at, change)| {
            let line = change.line;
            let (colour, end) = if change.removed { (tokens.danger, format!("\u{2212}{}", change.lines)) } else { (tokens.ok, format!("+{}", change.lines)) };
            let slots = Slots {
                icon: Some(bar(colour).into_any_element()),
                end: Some(end_text(end, colour, tokens).into_any_element()),
                on: change.on,
                ..Slots::new(change_label(change))
            };
            row(("code-change", at), slots, tokens)
                .tooltip(tip(change.text.clone()))
                .on_click(cx.listener(move |this, _, _, cx| this.send(CodeGesture::Click { target: CodeClick::Go { line } }, cx)))
                .into_any_element()
        }).collect::<Vec<_>>();
        let close = icon_button("code-outline-close", CLOSE, false, tokens)
            .tooltip(tip(CLOSE))
            .child(Glyph::Close.at(tokens.icon_sm))
            .on_click(cx.listener(|this, _, _, cx| this.send(CodeGesture::Click { target: CodeClick::Outline }, cx)))
            .into_any_element();
        v_flex()
            .id("code-outline")
            .test_support()
            .aria_label("Outline")
            .relative()
            .flex_shrink_0()
            .w(rem(width))
            .max_w(relative(at_most))
            .h_full()
            .min_h_0()
            .border_l_1()
            .border_color(tokens.line)
            .overflow_hidden()
            .child(panel_head("code-outline-head", HEADING, None, vec![close], tokens))
            // The rows scroll under the header, as `.code-outline` does on its own.
            .child(
                v_flex()
                    .id("code-outline-rows")
                    .test_support()
                    .flex_1()
                    .min_h_0()
                    .pt(rem(tokens.sp_8))
                    .pb(rem(tokens.sp_12))
                    // `.row` margin: 0 6px, so a picked row's ring stops short of the column's edge.
                    .px(rem(tokens.sp_6))
                    .overflow_y_scroll()
                    .when(page.symbols.is_empty(), |rows| {
                        rows.child(div().id("code-outline-none").test_support().aria_label(empty(page)).px(rem(14.0)).typed(tokens.type_small).text_color(tokens.fg_muted).child(empty(page)))
                    })
                    .children(symbols)
                    .when(!page.changes_here.is_empty(), |rows| {
                        rows.child(section::head("code-changes-head", SharedString::from(CHANGES), Some(page.changes_here.len()), None, tokens))
                    })
                    .children(changes),
            )
            // The grab zone along the column's left edge.
            .child(div().absolute().top_0().bottom_0().left_0().w(rem(tokens.sp_6)).child(self.outline_split.clone()))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn symbol(name: &str, kind: &'static str, method: bool) -> CodeOutlineRow {
        CodeOutlineRow { name: name.into(), kind, line: 1, on: false, changed: false, method }
    }

    /// The board nests methods under their impl: a method sits under the definition before it.
    #[test]
    fn a_method_nests_under_the_definition_before_it() {
        let symbols = [symbol("Rail", "type", false), symbol("Rail", "type", false), symbol("new", "fn", true), symbol("quiet", "fn", true), symbol("main", "fn", false)];
        let shape: Vec<_> = nesting(&symbols).iter().map(|entry| (entry.at, entry.methods.clone())).collect();
        assert_eq!(shape, [(0, vec![]), (1, vec![2, 3]), (4, vec![])]);
        assert_eq!(nesting(&[symbol("lone", "fn", true)]).len(), 1, "a method with nothing before it stands on its own");
    }

    /// The grammar tags a struct and its impl alike, so the second of a name is the impl (`i`).
    #[test]
    fn a_type_named_again_is_its_impl() {
        let symbols = [symbol("Rail", "type", false), symbol("Rail", "type", false), symbol("go", "fn", false)];
        assert_eq!(kind_of(&symbols, 0), (Kind::Type, "T".to_owned()));
        assert_eq!(kind_of(&symbols, 1), (Kind::Implementation, "i".to_owned()));
        assert_eq!(kind_of(&symbols, 2), (Kind::Function, "f".to_owned()));
    }

    #[test]
    fn a_run_of_lines_says_its_span() {
        let change = |line, lines| CodeChangeRow { line, text: String::new(), on: false, lines, removed: false };
        assert_eq!(change_label(&change(5, 1)), "Line 5");
        assert_eq!(change_label(&change(14, 9)), "Lines 14\u{2013}22");
    }
}
