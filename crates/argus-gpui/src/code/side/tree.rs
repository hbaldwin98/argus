//! The files column while neither list is open: the checkout and which of its files the tree lists,
//! then the tree itself (`scope`, `tree`, `code/stage.ts`). Core has ordered the rows, opened the
//! folders and marked what changed (`code::tree`, `checkouts::marks`); the rows are the kit's, a
//! chain of folders that hold only a folder is one row (`chain`) and the levels hang on guide
//! lines. The ignored top-level entries are held in one row at the foot, which names only what git
//! ignores and opens to nothing.

use argus_desktop_core::checkouts::fold::fold_ignored;
use argus_desktop_core::checkouts::marks::{self, Tone};
use argus_desktop_core::code::side::scope_tip;
use argus_desktop_core::code::stage::{CodeClick, CodeGesture, CodePage};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, AnyElement, App, Context, SharedString, TestSupportExt as _, Window};

use super::super::stage::CodeStage;
use super::chain::{lines, Line};
use crate::kit::ghosts::ghosts;
use crate::kit::icons::Glyph;
use crate::kit::row::{end_text, row, Slots};
use crate::kit::tip::tip;
use crate::kit::tree::fold_row;
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

    /// One drawn row, with the guide lines of the levels it hangs in on its left.
    fn tree_line(&self, line: &Line, page: &CodePage, under: &std::collections::HashMap<&str, usize>, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let entry = &page.tree[line.at()];
        let label = SharedString::from(page.tree[line.chain[0]].name.clone());
        let mut tail = None;
        if line.chain.len() > 1 {
            // `argus-gpui` then the faint `/src/chrome`.
            let rest: Vec<&str> = line.chain[1..].iter().map(|&at| page.tree[at].name.as_str()).collect();
            tail = Some(SharedString::from(format!("/{}", rest.join("/"))));
        }
        let path = entry.path.clone();
        let ignored = entry.ignored;
        let mark = marks::of_code(entry, under).map(|mark| {
            let colour = match mark.tone {
                Tone::Changed => tokens.changed,
                Tone::Untracked => tokens.fg_muted,
            };
            div()
                .id(SharedString::from(format!("mark-{}", entry.path)))
                .test_support()
                .aria_label(SharedString::from(mark.title.clone()))
                .tooltip(tip(mark.title.clone()))
                .child(end_text(mark.text, colour, tokens))
                .into_any_element()
        });
        let icon = div().text_color(tokens.fg_muted).child(if entry.folder { Glyph::FolderSm } else { Glyph::FileSm }.at(tokens.icon_xs)).into_any_element();
        let slots = Slots { chevron: entry.folder.then_some(entry.open), icon: Some(icon), tail, end: mark, on: entry.on, dim: ignored, ..Slots::new(label) };
        let row = row(("code-row", line.at()), slots, tokens)
            .tooltip(tip(entry.path.clone()))
            // An ignored entry is only named here: nothing under it is listed to open or read.
            .on_click(cx.listener(move |this, _, _, cx| {
                if !ignored {
                    this.send(CodeGesture::Click { target: CodeClick::Tree { path: path.clone() } }, cx);
                }
            }));
        if line.depth == 0 {
            return row.into_any_element();
        }
        // A level is 16px, and its guide is the line under its parent's chevron.
        h_flex()
            .flex_shrink_0()
            .items_stretch()
            .children((0..line.depth).map(|_| div().flex_none().w(rem(tokens.sp_16)).border_r_1().border_color(tokens.line)))
            .child(v_flex().flex_1().min_w_0().child(row))
            .into_any_element()
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
            let mut out: Vec<AnyElement> = lines(&page.tree, &folded.shown).iter().map(|line| self.tree_line(line, page, &under, tokens, cx)).collect();
            if !folded.held.is_empty() {
                out.push(fold_row("code-fold", &folded.label(), self.unfolded, toggle, tokens).into_any_element());
                if self.unfolded {
                    let held: Vec<AnyElement> = lines(&page.tree, &folded.held).iter().map(|line| self.tree_line(line, page, &under, tokens, cx)).collect();
                    out.push(v_flex().id(("code-fold", 1usize)).w_full().flex_shrink_0().opacity(tokens.tree_held_opacity).children(held).into_any_element());
                }
            }
            out
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
