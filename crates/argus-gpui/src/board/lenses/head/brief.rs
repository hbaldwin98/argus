//! An open feature's brief under its status line (BOARD-HEAD-3, -12): the brief in body type, two lines
//! of it until the whole is asked for, and the way into writing it.
//!
//! One line cut off after a sentence was a brief nobody could read, and the whole of a long one would
//! push the outline down the window; so two lines show, and Show all opens the rest in a box no taller
//! than `brief_max_h`, which scrolls. Show all is offered only when the brief runs past two lines at
//! the width its column was last laid out at, so the button never promises what it cannot show. Which
//! feature's brief is read whole is the window's own (`OpenFeature::brief_read`), as the legend is.

use std::cell::Cell;
use std::rc::Rc;

use argus_desktop_core::connection::page::FeatureView;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{canvas, div, font, prelude::*, AnyElement, Bounds, Entity, FontWeight, Pixels, SharedString, TestSupportExt as _, TextRun, Window};

use crate::board::feature_view::OpenFeature;
use crate::board::lenses::forms::edit::Editing;
use crate::kit::button::{button, Look};
use crate::kit::tip::tip;
use crate::style::Typed as _;
use crate::theme::scale::{pixels, rem};
use crate::theme::Tokens;

/// The lines a brief shows until it is read whole.
pub const LINES: usize = 2;

/// How many lines `text` takes in body type at `width`, a hard break starting a line as a wrap does.
pub fn lines(text: &str, width: Pixels, window: &Window, tokens: &Tokens) -> usize {
    if text.is_empty() {
        return 0;
    }
    let mut face = font(SharedString::from(tokens.font_ui));
    face.weight = FontWeight(tokens.type_body.weight);
    let run = TextRun { len: text.len(), font: face, color: gpui_kit::transparent_black(), background_color: None, underline: None, strikethrough: None };
    let size = pixels(tokens.type_body.size, window.rem_size());
    match window.text_system().shape_text(SharedString::from(text.to_owned()), size, &[run], Some(width), None) {
        Ok(shaped) => shaped.iter().map(|line| 1 + line.wrap_boundaries().len()).sum(),
        Err(_) => 0,
    }
}

/// Whether the brief says more than its two lines at the width its column was last laid out at; a
/// column not yet laid out says nothing more, and the frame after it is measured asks again.
pub fn runs_past(text: &str, area: &Rc<Cell<Bounds<Pixels>>>, window: &Window, tokens: &Tokens) -> bool {
    let width = area.get().size.width;
    width > Pixels::ZERO && lines(text, width, window, tokens) > LINES
}

/// Records the width the brief's column was laid out at, and draws the feature again on the next frame
/// when it has changed, so Show all comes and goes with the room the brief has.
fn measure(into: Rc<Cell<Bounds<Pixels>>>, feature: &Entity<OpenFeature>) -> impl IntoElement {
    let id = feature.entity_id();
    canvas(
        move |bounds, window, _| {
            let was = into.replace(bounds);
            if was.size.width != bounds.size.width {
                window.on_next_frame(move |_, cx| cx.notify(id));
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .size_full()
}

/// The way into writing a brief there is none of, at the end of the status line.
pub fn write(feature: &Entity<OpenFeature>, tokens: &Tokens) -> AnyElement {
    let opening = feature.clone();
    button("brief-write", "Write the brief", Look::Quiet, true, tokens)
        .tooltip(tip("Write the brief (B)"))
        .child("Write the brief")
        .on_click(move |_, window, cx| opening.update(cx, |feature, cx| feature.begin(Editing::Brief, window, cx)))
        .into_any_element()
}

/// The brief as a block under the status line: two lines, or the whole in a box that scrolls once it
/// is taller than `brief_max_h`; beside it Show all or Show less when there is more to show, and Edit.
pub fn block(view: &FeatureView, feature: &Entity<OpenFeature>, area: &Rc<Cell<Bounds<Pixels>>>, whole: bool, more: bool, tokens: &Tokens) -> AnyElement {
    let (toggling, opening) = (feature.clone(), feature.clone());
    let slug = view.slug.clone();
    let text = div().id("brief-text").test_support().typed(tokens.type_body).text_color(tokens.fg).child(SharedString::from(view.brief.clone()));
    let text = if whole { text.max_h(rem(tokens.brief_max_h)).overflow_y_scroll() } else { text.line_clamp(LINES).text_ellipsis() };
    let toggle = (whole || more).then(|| {
        let words = if whole { "Show less" } else { "Show all" };
        button("brief-toggle", words, Look::Quiet, true, tokens)
            .aria_expanded(whole)
            .child(words)
            .on_click(move |_, _, cx| toggling.update(cx, |feature, cx| feature.read_brief(&slug, !whole, cx)))
    });
    // Named apart from the feature menu's "Edit the brief", so a reader finds each by its own name.
    let edit = button("brief-edit-button", "Edit brief", Look::Quiet, true, tokens)
        .tooltip(tip("Edit the brief (B)"))
        .child("Edit")
        .on_click(move |_, window, cx| opening.update(cx, |feature, cx| feature.begin(Editing::Brief, window, cx)));
    h_flex()
        .id("feature-brief")
        .test_support()
        .aria_label(SharedString::from(view.brief.clone()))
        .items_start()
        .gap(rem(tokens.sp_12))
        .min_w_0()
        .child(v_flex().relative().flex_1().min_w_0().child(measure(area.clone(), feature)).child(text))
        .child(h_flex().flex_shrink_0().gap(rem(tokens.sp_4)).items_center().children(toggle).child(edit))
        .into_any_element()
}
