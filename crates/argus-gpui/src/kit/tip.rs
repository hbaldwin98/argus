//! A hover tooltip for a control: its words on the kit's card, drawn in the theme.
//!
//! The page says these with `title`; the kit's tooltip is the same words on a card drawn in the
//! theme. A control is given one with `.tooltip(tip("Refresh"))`, so no surface builds the kit's
//! tooltip itself, and the words that name a key are `caps::titled`'s.

use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::{AnyView, App, SharedString, Window};

/// The tooltip builder for `words`, as a control's `.tooltip` takes it.
pub fn tip(words: impl Into<SharedString>) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let words = words.into();
    move |window, cx| Tooltip::new(words.clone()).build(window, cx)
}

