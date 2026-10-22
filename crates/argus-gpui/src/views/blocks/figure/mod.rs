//! A drawing a view block makes (a graph, a sequence diagram) that opens larger over the window, to
//! be zoomed about the pointer and dragged, with its source beside it when it has one (FIGURE-1 to
//! FIGURE-7). The page's `views/blocks/figure.ts`.
//!
//! A block hands over what [`Figure`] holds (a name, the size it was laid out at, a way to draw it at
//! any scale) and gets back its drawing with the corner button and the click that open it. What the
//! larger view shows is drawn afresh from the block, so what is pressable in the drawing still is;
//! it is the window's alone, like a tab chosen, and nothing about it reaches Rust. While it is up it
//! holds every key (`keys/layers/figure`). The numbers are core's (`views::blocks::zoom`).

pub mod bar;
pub mod larger;
pub mod open;
pub mod stage;

use std::rc::Rc;

use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, AnyElement, SharedString};

use crate::kit::icon_button::icon_button;
use crate::kit::icons::Glyph;
use crate::kit::tip::tip;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// What a block hands over to be opened larger.
pub struct Figure {
    /// What the larger view's bar says it is.
    pub title: String,
    /// What it was drawn from, shown beside it on request.
    pub source: Option<String>,
    /// The size it was laid out at, in the page's pixels.
    pub size: (f32, f32),
    /// Draws it at `scale` times that size; called again each time it is zoomed.
    pub draw: Rc<dyn Fn(f32) -> AnyElement>,
}

/// `inline` with the button that opens it larger in its corner, shown while the pointer is over the
/// drawing or the button has the keys; a press on the drawing opens it too, unless it lands on
/// something of its own to press (a box that opens a view, a link, a button), which takes the press
/// first.
pub fn expandable(inline: AnyElement, figure: Figure, key: &str, tokens: &Tokens) -> AnyElement {
    let figure = Rc::new(figure);
    let group = SharedString::from(format!("{key}-fig"));
    let (by_click, by_button) = (figure.clone(), figure.clone());
    let open_larger = icon_button(SharedString::from(format!("{key}-open")), format!("Open {} larger", figure.title), false, tokens)
        .absolute()
        .top(rem(tokens.sp_6))
        .right(rem(tokens.sp_6))
        .bg(tokens.bg_surface)
        .border_1()
        .border_color(tokens.line)
        .opacity(0.0)
        .group_hover(group.clone(), |style| style.opacity(1.0))
        .focus_visible(|style| style.opacity(1.0))
        .tooltip(tip("Open larger"))
        .on_click(move |_, window, cx| open::open(by_button.clone(), window, cx))
        .child(Glyph::Expand.at(tokens.icon_sm));
    div()
        .id(group.clone())
        .test_support()
        .group(group)
        .relative()
        .min_w_0()
        .child(
            div()
                .id(SharedString::from(format!("{key}-inline")))
                .test_support()
                .min_w_0()
                .cursor_pointer()
                .on_click(move |_, window, cx| open::open(by_click.clone(), window, cx))
                .child(inline),
        )
        .child(open_larger)
        .into_any_element()
}
