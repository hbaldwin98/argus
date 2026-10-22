//! The box every selectable line of the rail is made of, the marks that say a line is open or shut,
//! and what a row is drawn against.

use gpui_kit::component::h_flex;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{prelude::*, Hsla, Role, SharedString};

use crate::kit::icons::Glyph;
use crate::kit::tip::tip;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// What every row of one drawing of the rail is drawn against.
#[derive(Debug, Clone, Copy)]
pub(super) struct Ctx {
    /// The daemon is gone: the list is dimmed and a press sends nothing.
    pub held: bool,
    /// A stage other than the Workspace is up, so the pane on screen is not the selection.
    pub offstage: bool,
    /// The rail is narrow: stages by icon, panes by initials, every name a tooltip.
    pub icons: bool,
    /// The window's height, for what is held to a share of it.
    pub height: f32,
}

/// A selectable line of the rail: its box, with the colours and padding left
/// to the kind of row, since the page gives each its own.
pub(super) type Line = gpui_kit::base::ObservedElement<gpui_kit::Stateful<gpui_kit::Div>>;

pub(super) fn line(id: impl Into<gpui_kit::ElementId>, height: f32, tokens: &Tokens) -> Line {
    h_flex()
        .id(id)
        .role(Role::Button)
        .test_support()
        .h(rem(height))
        .flex_shrink_0()
        .items_center()
        .rounded(rem(tokens.r_sm))
        .cursor_pointer()
}

/// The turn of a chevron is `--t-fast`'s neighbour, 0.15s, in the page; here
/// the mark swaps, since the glyph set has one chevron per direction.
pub(super) fn chevron(id: String, open: bool, width: f32, colour: Hsla, size: f32) -> Line {
    h_flex()
        .id(id)
        .test_support()
        .aria_expanded(open)
        .items_center()
        .justify_center()
        .w(rem(width))
        .flex_shrink_0()
        .text_color(colour)
        .child(if open { Glyph::Down } else { Glyph::Right }.at(size))
}

/// `.repo .icon`, `.co .icon`: a small button at the end of a row, drawn only while its row is
/// pointed at or lit (the row's `group` says which). It is named `label` to a reader and says `words`
/// on hover. The glyph is the caller's child.
pub(super) fn quiet_button(id: impl Into<gpui_kit::ElementId>, label: impl Into<SharedString>, words: impl Into<SharedString>, tokens: &Tokens) -> Line {
    let (fill, text) = (tokens.fill, tokens.fg);
    line(id, tokens.icon_box, tokens)
        .aria_label(label)
        .tooltip(tip(words))
        .w(rem(tokens.icon_box))
        .justify_center()
        .text_color(tokens.fg_muted)
        .hover(move |style| style.bg(fill).text_color(text))
}
