//! A checkout's file tree as one widget, drawn by the Code stage and the Files panel alike so a
//! checkout looks the same in both (`kit/file_tree.ts`): a chevron, a folder or file glyph, the
//! name, its change mark, and the ignored top-level folders held in one row at the foot.
//!
//! Each surface keeps what is its own, what a click asks, its keys and the buttons on a row, and
//! puts them on the row this returns; core has already ordered the rows, said which are open
//! and which fold (`checkouts::fold`) and what each mark says (`checkouts::marks`).

use argus_desktop_core::checkouts::fold::Folded;
use argus_desktop_core::checkouts::marks::{Mark, Tone};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, AnyElement, App, ElementId, Role, SharedString, TestSupportExt as _, Window};

use crate::kit::icon_button::Node;
use crate::kit::icons::Glyph;
use crate::kit::selected::selected;
use crate::kit::tip::tip;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// What a row says of the entry it is for.
pub struct Row<'a> {
    /// The path, which is the row's tooltip and, with its surface, its name for a reader.
    pub path: &'a str,
    pub name: &'a str,
    pub depth: usize,
    pub dir: bool,
    pub open: bool,
    /// The file being read, or the row the keys are at.
    pub on: bool,
    pub ignored: bool,
    /// The tree holds the keys, so the row they are at wears the ring (`.cf-tree:focus-visible .cf-row.on`).
    pub ringed: bool,
    pub mark: Option<Mark>,
}

fn chevron(open: bool, dir: bool, tokens: &Tokens) -> impl IntoElement {
    // `.cf-chev`: a slot as wide as the glyph, so names line up whether or not a row has one.
    div()
        .flex_shrink_0()
        .w(rem(tokens.icon_sm))
        .flex()
        .justify_center()
        .text_color(tokens.fg_faint)
        .when(dir, |slot| slot.child(if open { Glyph::Down } else { Glyph::Right }.at(tokens.icon_sm)))
}

fn mark(mark: &Mark, path: &str, tokens: &Tokens) -> impl IntoElement {
    let colour = match mark.tone {
        Tone::Changed => tokens.changed,
        Tone::Untracked => tokens.fg_muted,
    };
    // `.cf-mark`: mono, 10px and bold, right-aligned in 14px.
    div()
        .id(SharedString::from(format!("mark-{path}")))
        .test_support()
        .aria_label(SharedString::from(mark.title.clone()))
        .flex_shrink_0()
        .min_w(rem(tokens.tree_mark_w))
        .flex()
        .justify_end()
        .font_family(SharedString::from(tokens.font_mono))
        .text_size(rem(tokens.fs_xs))
        .weighted(tokens.weight_bold)
        .text_color(colour)
        .tooltip(tip(mark.title.clone()))
        .child(SharedString::from(mark.text.clone()))
}

/// `.cf-row`: one entry of the tree, `--row-h` tall, indented a level at a time. The caller gives
/// it what a press does and any `extra` controls, which sit between the name and the mark.
pub fn row(id: impl Into<ElementId>, entry: &Row, extra: Option<AnyElement>, tokens: &Tokens) -> Node {
    let (fill, hover_on) = (tokens.fill, tokens.selected_hover);
    let faint = entry.ignored;
    let row = h_flex()
        .id(id)
        .role(Role::TreeItem)
        .test_support()
        .aria_label(SharedString::from(entry.name.to_owned()))
        .aria_selected(entry.on)
        .tooltip(tip(entry.path.to_owned()))
        .h(rem(tokens.row_h))
        .flex_shrink_0()
        .w_full()
        .items_center()
        .gap(rem(tokens.sp_6))
        .rounded(rem(tokens.r_sm))
        .pl(rem(tokens.sp_4 + entry.depth as f32 * tokens.tree_indent))
        .pr(rem(tokens.sp_8))
        .text_size(rem(tokens.fs_md))
        .text_color(tokens.fg)
        .cursor_pointer()
        .when(!entry.on, |row| row.hover(move |style| style.bg(fill)))
        .when(entry.on, |row| row.hover(move |style| style.bg(hover_on)));
    let row = selected(row, entry.on, tokens);
    let row = if entry.on && entry.ringed { row.border_color(tokens.ring) } else { row };
    let row = if entry.dir { row.aria_expanded(entry.open) } else { row };
    row.child(chevron(entry.open, entry.dir, tokens))
        .child(
            div()
                .flex_shrink_0()
                .text_color(if faint { tokens.fg_faint } else { tokens.fg_muted })
                .child(if entry.dir { Glyph::FolderSm } else { Glyph::FileSm }.at(tokens.icon_xs)),
        )
        .child(div().flex_1().min_w_0().truncate().text_color(if faint { tokens.fg_faint } else { tokens.fg }).child(SharedString::from(entry.name.to_owned())))
        .children(extra)
        .children(entry.mark.as_ref().map(|m| mark(m, entry.path, tokens).into_any_element()))
}

/// `.ft-fold`: the one row that stands for the ignored top-level folders, which opens to show
/// them. `label` is core's (`4 ignored · .argus, target`).
pub fn fold_row(id: impl Into<ElementId>, label: &str, open: bool, on_press: impl Fn(&mut Window, &mut App) + 'static, tokens: &Tokens) -> Node {
    let hover = tokens.fg_muted;
    h_flex()
        .id(id)
        .role(Role::Button)
        .test_support()
        .aria_label(SharedString::from(label.to_owned()))
        .aria_expanded(open)
        .tooltip(tip("Ignored by git"))
        .w_full()
        .flex_shrink_0()
        .h(rem(tokens.row_h))
        .mt(rem(tokens.sp_6))
        .pl(rem(tokens.sp_4))
        .pr(rem(tokens.sp_8))
        .gap(rem(tokens.sp_6))
        .items_center()
        .border_t_1()
        .border_color(tokens.line)
        .typed(tokens.type_small)
        .text_color(tokens.fg_faint)
        .cursor_pointer()
        .hover(move |style| style.text_color(hover))
        .on_click(move |_, window, cx| on_press(window, cx))
        .child(div().flex_shrink_0().w(rem(tokens.icon_sm)).flex().justify_center().child(if open { Glyph::Down } else { Glyph::Right }.at(tokens.icon_sm)))
        .child(div().min_w_0().truncate().child(SharedString::from(label.to_owned())))
}

/// The tree's rows with the ignored ones folded: `make` draws the row at an index, and the fold
/// row and, while it is open, the rows it holds follow the others, dimmed.
pub fn rows(
    folded: &Folded,
    open: bool,
    id: &'static str,
    make: &mut dyn FnMut(usize) -> AnyElement,
    on_fold: impl Fn(&mut Window, &mut App) + 'static,
    tokens: &Tokens,
) -> Vec<AnyElement> {
    let mut out: Vec<AnyElement> = folded.shown.iter().map(|&at| make(at)).collect();
    if folded.held.is_empty() {
        return out;
    }
    out.push(fold_row(id, &folded.label(), open, on_fold, tokens).into_any_element());
    if open {
        let held: Vec<AnyElement> = folded.held.iter().map(|&at| make(at)).collect();
        out.push(v_flex().id((id, 1usize)).w_full().flex_shrink_0().opacity(tokens.tree_held_opacity).children(held).into_any_element());
    }
    out
}
