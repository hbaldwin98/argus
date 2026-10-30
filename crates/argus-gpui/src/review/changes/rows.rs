//! A file's row in the Changes pane's lists, and a folder's: what each says, and what a press on
//! it asks. A flat row says name first, its folder after it faint and cut from the left
//! (`kit::path`), and a row in the tree is the kit's one row (`kit::row`) inside its folders' guide
//! lines. At the end of both, the counts give way to the stage button while the row is hovered.
//!
//! A click, a double-click and a drag are the list's (`lists`); the folder's open or closed is the
//! window's own and is changed here, never sent.

use argus_desktop_core::review::staging::{SectionFile, StageGesture};
use argus_desktop_core::connection::page::Call;
use gpui_kit::component::h_flex;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, AnyElement, Context, MouseButton, MouseDownEvent, SharedString};

use crate::kit::counts::counts;
use crate::kit::icons::Glyph;
use crate::kit::path::path_label;
use crate::kit::reveal;
use crate::kit::row::{end_text, nest, row, Slots};
use crate::kit::tip::tip;
use crate::review::changes::flat::FILE_ROW;
use crate::review::changes::lines::Line;
use crate::review::changes::lists::{ghost, status, title, Carried};
use crate::review::changes::side::Files;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// The status letter in the mono face, on the colour of what happened to the file.
pub(super) fn letter(id: impl Into<gpui_kit::ElementId>, letter: &'static str, colour: gpui_kit::Hsla, said: &'static str, tokens: &Tokens) -> impl IntoElement {
    div()
        .id(id)
        .flex_none()
        .w(rem(tokens.icon_xs))
        .text_center()
        .font_family(SharedString::from(tokens.font_mono))
        .text_size(rem(tokens.fs_xs))
        .weighted(tokens.weight_semibold)
        .text_color(colour)
        .child(letter)
        .tooltip(tip(said))
}

/// `drawn` one guide line in for each folder `depth` is inside.
fn nested(mut drawn: AnyElement, depth: usize, at: usize, tokens: &Tokens) -> AnyElement {
    for level in 0..depth {
        drawn = nest(("review-nest", at * 32 + level), vec![drawn], tokens).into_any_element();
    }
    drawn
}

impl Files {
    /// A folder of the tree: its chevron, folder icon, name and how many files are under it. A
    /// press opens or closes it.
    pub(super) fn folder_row(&self, at: usize, line: &Line, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let Line::Folder { list, label, path, depth, open, files } = line else { return div().into_any_element() };
        let (list, depth) = (*list, *depth);
        let slots = Slots {
            chevron: Some(*open),
            icon: Some(Glyph::Folder.at(14.0).into_any_element()),
            end: Some(end_text(files.to_string(), tokens.fg_faint, tokens).into_any_element()),
            ..Slots::new(SharedString::from(label.clone()))
        };
        let path = path.clone();
        let drawn = row(("review-folder", at), slots, tokens)
            .tooltip(tip(SharedString::from(path.trim_end_matches('/').to_owned())))
            .on_click(cx.listener(move |this, _, _, cx| {
                let key = (list, path.clone());
                if !this.closed.remove(&key) {
                    this.closed.insert(key);
                }
                cx.notify();
            }))
            .into_any_element();
        nested(drawn, depth, at, tokens)
    }

    /// One file of a list: its status, name, folder (a flat row's own; a tree's folders say it),
    /// and counts, and, hovered or lit, the move across where the counts were.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn change(&self, id: usize, at: usize, file: &SectionFile, staged: bool, depth: Option<usize>, talked: &[(String, usize)], tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let (mark, colour, said) = status(file.file.kind, tokens);
        let path = file.file.path.clone();
        let carried = Carried { path: path.clone(), staged };
        let moving = self.carried().is_some_and(|c| *c == carried) && self.drag.is_dragging();
        let group = SharedString::from(format!("review-change-{id}"));
        let across = if staged { "Unstage" } else { "Stage" };
        let move_path = path.clone();
        let waiting = talked.iter().find(|(p, _)| *p == path).map(|(_, n)| *n);
        let hint = if staged { "Double-click or drag to Changes to unstage" } else { "Double-click or drag to Staged to stage" };
        let status_mark = letter(("review-change-status", id), mark, colour, said, tokens);
        let end = h_flex()
            .flex_none()
            .gap(rem(tokens.sp_6))
            .items_center()
            .when(file.partly, |end| {
                end.child(
                    div()
                        .id(("review-change-partly", id))
                        .test_support()
                        .aria_label("Partly staged")
                        .flex_shrink_0()
                        .size(rem(6.0))
                        .rounded_full()
                        .bg(tokens.yellow)
                        .tooltip(tip("Partly staged")),
                )
            })
            .when_some(waiting, |end, n| {
                let words = SharedString::from(format!("{n} comment{} waiting in your review", if n == 1 { "" } else { "s" }));
                end.child(
                    h_flex()
                        .id(("review-change-talk", id))
                        .test_support()
                        .aria_label(words.clone())
                        .flex_shrink_0()
                        .gap(rem(3.0))
                        .items_center()
                        .text_color(tokens.accent)
                        .font_family(SharedString::from(tokens.font_mono))
                        .text_size(rem(11.0))
                        .tooltip(tip(words))
                        .child(Glyph::Talk.at(12.0))
                        .child(SharedString::from(n.to_string())),
                )
            })
            // `.nums` and `.stage` share the row's end: the counts until the row is hovered or lit,
            // then the move across.
            .child(
                h_flex()
                    .flex_shrink_0()
                    .items_center()
                    .when(!file.on, |end| {
                        end.child(reveal::off_hover(group.clone(), div().text_size(rem(tokens.fs_xs)).child(counts(file.file.added, file.file.removed, tokens))))
                    })
                    .child(reveal::on_hover(
                        group.clone(),
                        file.on,
                        ghost(("review-change-move", id), SharedString::from(format!("{across} {move_path}")), tokens)
                            .tooltip(tip(SharedString::from(format!("{across} file"))))
                            .child(if staged { Glyph::Minus.at(14.0) } else { Glyph::Plus.at(14.0) })
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.ask(Call::Stage { gesture: StageGesture::File { staged, path: move_path.clone() } }, cx);
                            })),
                    )),
            );
        let label = SharedString::from(format!("{} {}", title(if staged { "staged" } else { file.file.kind }), path));
        let down = path.clone();
        let drawn = match depth {
            Some(_) => {
                let slots = Slots { icon: Some(status_mark.into_any_element()), end: Some(end.into_any_element()), on: file.on, ..Slots::new(SharedString::from(file.file.name.clone())) };
                row(("review-change", id), slots, tokens).aria_label(label)
            }
            None => {
                let dir = file.file.dir.trim_end_matches('/');
                h_flex()
                    .id(("review-change", id))
                    .test_support()
                    .aria_selected(file.on)
                    .aria_label(label)
                    .flex_shrink_0()
                    .h(rem(FILE_ROW))
                    .mx(rem(tokens.sp_6))
                    .pl(rem(tokens.sp_8))
                    .pr(rem(tokens.sp_6))
                    .gap(rem(tokens.sp_8))
                    .items_center()
                    .rounded(rem(tokens.r_sm))
                    .border_1()
                    .border_color(if file.on { tokens.line_strong } else { gpui_kit::transparent_black() })
                    .typed(tokens.type_body)
                    .text_color(if file.on { tokens.fg } else { tokens.fg_muted })
                    .cursor_pointer()
                    .when(file.on, |row| row.bg(tokens.bg_selected))
                    .when(!file.on, |row| row.hover(|style| style.bg(tokens.fill.opacity(0.6)).text_color(tokens.fg)))
                    .child(status_mark)
                    .child(div().flex_1().min_w_0().overflow_hidden().child(path_label(("review-change-path", id), &file.file.name, dir, tokens)))
                    .child(end)
            }
        };
        let drawn = drawn
            .group(group)
            .when(moving, |row| row.opacity(0.45))
            .tooltip(tip(SharedString::from(format!("{path}\n{hint}"))))
            // A second press of a double-click moves the file; a first is held, to be a click or a drag.
            .on_mouse_down(MouseButton::Left, cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                if event.click_count >= 2 {
                    this.drag.cancel();
                    this.ask(Call::Stage { gesture: StageGesture::File { staged, path: down.clone() } }, cx);
                } else {
                    this.drag.press(event.position, carried.clone());
                }
            }))
            .into_any_element();
        nested(drawn, depth.unwrap_or(0), at, tokens)
    }
}
