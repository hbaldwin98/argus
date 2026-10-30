//! The box every selectable line of the rail is made of, the quiet button at the end of one,
//! and what a row is drawn against.

use gpui_kit::component::h_flex;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{prelude::*, Role, SharedString};

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
