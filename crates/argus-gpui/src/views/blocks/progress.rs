//! A meter: how far along something is, as a fill in a track in the tone's colour, and the compact
//! one a progress value draws in a cell or a field with its percentage beside it.

use argus_desktop_core::views::values::{fraction, percent};
use argus_protocol::Tone;
use gpui_kit::component::h_flex;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, relative, ElementId, Role, SharedString};

use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;
use crate::views::chip::tone_colour;

/// `.vmeter`: a fill of `share` (0 to 1) in a track that is the tone at `share_meter_track`, both as tall as
/// `--meter-h`. It is as wide as what holds it.
pub fn meter(share: f32, tone: Tone, tokens: &Tokens) -> impl IntoElement {
    let colour = tone_colour(tone, tokens);
    div()
        .h(rem(tokens.meter_h))
        .w_full()
        .overflow_hidden()
        .rounded(rem(tokens.r_pill))
        .bg(colour.opacity(tokens.share_meter_track))
        .child(div().h_full().w(relative(share.clamp(0.0, 1.0))).rounded(rem(tokens.r_pill)).bg(colour))
}

/// `.vprogress-inline`: a small meter and the whole percentage, for a cell or a field. An agent's
/// overshoot or a zero max is not a broken bar.
pub fn progress_value(id: impl Into<ElementId>, value: f64, max: f64, tone: Tone, tokens: &Tokens) -> impl IntoElement {
    let said = format!("{}%", percent(value, max));
    h_flex()
        .id(id)
        .role(Role::ProgressIndicator)
        .test_support()
        .aria_label(said.clone())
        .items_center()
        .gap(rem(tokens.sp_8))
        .child(div().w(rem(tokens.meter_sm_w)).flex_shrink_0().child(meter(fraction(value, max), tone, tokens)))
        .child(div().font_family(tokens.font_mono).text_size(rem(tokens.fs_sm)).weighted(tokens.weight_regular).child(said))
}

/// `.vprogress`: the label and the agent's own note (`412 / 1,000`) over a meter in the tone's
/// colour.
pub fn progress_block(label: &str, value: f64, max: f64, tone: Tone, note: Option<&str>, key: &str, tokens: &Tokens) -> impl IntoElement {
    let said = note.map_or_else(|| format!("{}%", percent(value, max)), str::to_owned);
    gpui_kit::component::v_flex()
        .id(SharedString::from(format!("{key}-progress")))
        .role(Role::ProgressIndicator)
        .test_support()
        .aria_label(label.to_owned())
        .aria_value(said)
        .gap(rem(tokens.sp_4))
        .child(
            h_flex()
                .justify_between()
                .gap(rem(tokens.sp_8))
                .text_size(rem(tokens.fs_sm))
                .child(SharedString::from(label.to_owned()))
                .children(note.map(|note| div().font_family(tokens.font_mono).text_color(tokens.fg_muted).child(SharedString::from(note.to_owned())))),
        )
        .child(meter(fraction(value, max), tone, tokens))
}
