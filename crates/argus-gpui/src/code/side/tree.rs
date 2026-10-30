//! The files column while neither list is open: the checkout and which of its files the tree lists,
//! then the tree itself (`scope`, `tree`, `code/stage.ts`). Core has ordered the rows, opened the
//! folders and marked what changed (`code::tree`, `checkouts::marks`); the ignored top-level
//! entries are held in one row at the foot, which names only what git ignores and opens to nothing.

use argus_desktop_core::checkouts::fold::fold_ignored;
use argus_desktop_core::checkouts::marks;
use argus_desktop_core::code::side::scope_tip;
use argus_desktop_core::code::stage::{CodeClick, CodeGesture, CodePage};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, AnyElement, App, Context, SharedString, TestSupportExt as _, Window};

use super::super::stage::CodeStage;
use crate::kit::ghosts::ghosts;
use crate::kit::tip::tip;
use crate::kit::tree::{self, Row};
use crate::theme::scale::rem;
use crate::theme::Tokens;

impl CodeStage {
    /// `.code-tools`: the checkout's name. Which files the tree lists is the title bar's to choose
    /// (`stage::tabs`).
    fn scope(&self, page: &CodePage, tokens: &Tokens) -> impl IntoElement {
        h_flex()
            .id("code-scope-row")
            .test_support()
            .flex_shrink_0()
            .flex_wrap()
            .items_center()
            .gap(rem(tokens.sp_6))
            .child(
                div()
                    .id("code-scope")
                    .test_support()
                    .aria_label(SharedString::from(page.name.clone()))
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_family(SharedString::from(tokens.font_mono))
                    .text_size(rem(tokens.fs_sm))
                    .text_color(tokens.fg_muted)
                    .tooltip(tip(scope_tip(page).to_owned()))
                    .child(SharedString::from(page.name.clone())),
            )
    }

    pub(super) fn tree(&self, page: &CodePage, _window: &mut Window, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let folded = fold_ignored(&page.tree);
        let under = marks::changed_under(page.changed_files.iter().map(|f| f.path.as_str()));
        let me = cx.weak_entity();
        let toggle = move |_: &mut Window, cx: &mut App| {
            me.update(cx, |stage, cx| {
                stage.unfolded = !stage.unfolded;
                cx.notify();
            })
            .ok();
        };
        let rows: Vec<AnyElement> = if page.tree_loading {
            vec![ghosts("code-loading", &[0.6, 0.8, 0.5, 0.7], tokens).into_any_element()]
        } else {
            let mut make = |at: usize| {
                let row = &page.tree[at];
                let entry = Row { path: &row.path, name: &row.name, depth: row.depth, dir: row.folder, open: row.open, on: row.on, ignored: row.ignored, ringed: false, mark: marks::of_code(row, &under) };
                let path = row.path.clone();
                let ignored = row.ignored;
                tree::row(("code-row", at), &entry, None, tokens)
                    // An ignored entry is only named here: nothing under it is listed to open or read.
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !ignored {
                            this.send(CodeGesture::Click { target: CodeClick::Tree { path: path.clone() } }, cx);
                        }
                    }))
                    .into_any_element()
            };
            tree::rows(&folded, self.unfolded, "code-fold", &mut make, toggle, tokens)
        };
        v_flex()
            .flex_1()
            .min_h_0()
            .gap(rem(tokens.sp_6))
            .child(self.scope(page, tokens))
            .child(
                v_flex()
                    .id("code-list")
                    .test_support()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.tree_scroll)
                    .gap(rem(tokens.hairline))
                    .children(rows),
            )
            .into_any_element()
    }
}
