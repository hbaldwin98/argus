//! What fills the index before there are features to list: a card saying no repository is chosen,
//! six loading bars until the board arrives, and a card for a repository that has none yet
//! (`drawFeatures` of `feature_stage.ts`).

use argus_desktop_core::board::capture_note::CaptureGesture;
use argus_desktop_core::connection::page::Call;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, SharedString, TestSupportExt as _};

use crate::board::ctx::Ctx;
use crate::kit::button::{button, Look};
use crate::kit::ghosts::ghosts;
use crate::kit::icon_button::Node;
use crate::kit::icons::Glyph;
use crate::kit::keycap::keycap;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// Six bars of the page's widths.
const LOADING: [f32; 6] = [0.5, 0.8, 0.65, 0.75, 0.55, 0.7];

pub fn loading(tokens: &Tokens) -> impl IntoElement {
    div().id("features-loading").test_support().aria_label("Loading").flex_1().child(ghosts("features-ghosts", &LOADING, tokens))
}

/// `.empty > .empty-body`: what is said, centred in the room there is.
fn body(id: &'static str, tokens: &Tokens) -> Node {
    v_flex().id(id).test_support().size_full().flex_1().items_center().justify_center().gap(rem(tokens.sp_16)).p(rem(tokens.sp_24)).text_center()
}

/// No repository is chosen.
pub fn none_chosen(tokens: &Tokens) -> impl IntoElement {
    body("features-none", tokens)
        .child(div().text_color(tokens.fg_faint).child(Glyph::Folder.at(tokens.icon_lg)))
        .child(div().id("features-none-title").test_support().aria_label("No repository").typed(tokens.type_title).text_color(tokens.fg).child("No repository"))
        .child(div().text_color(tokens.fg_muted).child("Choose one on the left."))
}

/// The repository has no features: how they are made, and a button to write one.
pub fn nothing(repository: &str, ctx: &Ctx, tokens: &Tokens) -> impl IntoElement {
    let mono = SharedString::from(tokens.font_mono);
    let ctx = ctx.clone();
    body("features-nothing", tokens)
        .child(div().text_color(tokens.fg_faint).child(Glyph::Folder.at(tokens.icon_lg)))
        .child(div().id("features-nothing-title").test_support().aria_label(SharedString::from(format!("No features in {repository} yet"))).typed(tokens.type_title).text_color(tokens.fg).child(SharedString::from(format!("No features in {repository} yet"))))
        .child(
            h_flex()
                .flex_wrap()
                .justify_center()
                .items_center()
                .text_color(tokens.fg_muted)
                .child("Agents open them with\u{a0}")
                .child(div().font_family(mono.clone()).text_color(tokens.fg).child("argus-hook feature open"))
                .child(",\u{a0}or write one as a note with\u{a0}")
                .child(keycap("N", mono, tokens))
                .child("."),
        )
        .child(button("features-new", "New feature", Look::Primary, false, tokens).child("New feature").on_click(move |_, _, cx| ctx.send(Call::Capture { gesture: CaptureGesture::Open }, cx)))
}
