//! The Code stage's files column: the one field over it, and under it the tree with its scope row,
//! the go-to-file matches, or the search's options, summary and hits (`side`, `code/stage.ts`).
//!
//! Which of the three it is, what the field shows, and every sentence is core's (`code::side`); a
//! press sends its gesture and the list changes when the session's next page says it did. The
//! column keeps where each list was scrolled and brings the row the person moved to into view.

mod chain;
mod find;
mod jump;
mod search;
mod typed;
mod tree;

pub(in crate::code) use find::wire as wire_find;
pub(in crate::code) use search::wire as wire_glob;

use argus_desktop_core::checkouts::fold::fold_ignored;
use argus_desktop_core::code::side::{body, Body};
use argus_desktop_core::code::stage::CodePage;
use gpui_kit::component::v_flex;
use gpui_kit::{prelude::*, AnyElement, Context, Window, TestSupportExt as _};

use super::stage::CodeStage;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// Which list the column scrolls, and which of its children is lit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lit {
    Tree(usize),
    Jump(usize),
    Hit(usize),
}

/// The child of the open list that the cursor is on, to scroll to: the lit row of the tree, the
/// go-to list's `at`, or the selected hit counted across the headers and hits shown.
pub fn lit_child(page: &CodePage, unfolded: bool) -> Option<Lit> {
    match body(page) {
        Body::Tree => {
            let folded = fold_ignored(&page.tree);
            let at = page.tree.iter().position(|row| row.on)?;
            let drawn = chain::lines(&page.tree, &folded.shown);
            match drawn.iter().position(|line| line.chain.contains(&at)) {
                Some(shown) => Some(Lit::Tree(shown)),
                None => unfolded.then_some(Lit::Tree(drawn.len() + 1)),
            }
        }
        Body::Jump => page.jump.as_ref()?.at.map(Lit::Jump),
        Body::Search => {
            let search = page.search.as_ref().filter(|search| search.current)?;
            let mut child = 0;
            for file in &search.files {
                child += 1;
                if !file.open {
                    continue;
                }
                for hit in &file.hits {
                    if hit.on {
                        return Some(Lit::Hit(child));
                    }
                    child += 1;
                }
            }
            None
        }
    }
}

impl CodeStage {
    /// `.code-tree`: the files column, 8px in with 6 between its parts.
    pub(super) fn side(&self, page: &CodePage, window: &mut Window, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let list = match body(page) {
            Body::Search => self.search(page, tokens, cx),
            Body::Jump => self.jump(page, tokens, cx),
            Body::Tree => self.tree(page, window, tokens, cx),
        };
        v_flex()
            .id("code-tree")
            .test_support()
            .aria_label("Files in the checkout")
            .size_full()
            .min_h_0()
            .p(rem(tokens.sp_8))
            .gap(rem(tokens.sp_6))
            .child(self.find(page, tokens, cx))
            .child(list)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests;
