//! A section of the peek: a label over the lines of what needs you, what is in flight, what is
//! next and what was last decided (`lines` of `feature_stage.ts`).
//!
//! A line is a mark, its label and its text, with an aside under it in small type. A pane that
//! needs you comes as `◆` with its status as the aside: one row of icon, title and the shared
//! pill, without the `?` that belongs to an open decision.

use argus_desktop_core::connection::page::Line;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, Hsla, SharedString, TestSupportExt as _};

use crate::board::cells::meta;
use crate::kit::icons::Glyph;
use crate::kit::pill::{status_pill, status_word};
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// The label Rust gives a pane that needs you instead of a task number.
const AGENT_MARK: &str = "\u{25c6}";

/// How a section colours itself: what asks for you is attention throughout, work in hand marks
/// attention but keeps its task number quiet, and only a settled decision's label reads as a ref.
pub struct Tone {
    pub mark: Hsla,
    pub head: Hsla,
    pub label: Hsla,
    pub text: Option<Hsla>,
}

impl Tone {
    pub fn needs_you(tokens: &Tokens) -> Self {
        Self { mark: tokens.attention, head: tokens.attention, label: tokens.attention, text: None }
    }

    pub fn in_flight(tokens: &Tokens) -> Self {
        Self { mark: tokens.attention, head: tokens.fg_faint, label: tokens.fg_faint, text: None }
    }

    pub fn next_up(tokens: &Tokens) -> Self {
        Self { mark: tokens.fg_faint, head: tokens.fg_faint, label: tokens.fg_faint, text: None }
    }

    pub fn last_decided(tokens: &Tokens) -> Self {
        Self { mark: tokens.ok, head: tokens.fg_faint, label: tokens.ref_, text: Some(tokens.fg_muted) }
    }
}

/// The section `label` over `items`, or nothing when there are none.
pub fn section(label: &'static str, items: &[Line], mark: &'static str, tone: &Tone, tokens: &Tokens) -> Option<impl IntoElement> {
    if items.is_empty() {
        return None;
    }
    let mono = SharedString::from(tokens.font_mono);
    Some(
        v_flex()
            .id(SharedString::from(format!("peek-{}", label.to_lowercase().replace(' ', "-"))))
            .test_support()
            .aria_label(label)
            .gap(rem(tokens.sp_4))
            .child(div().typed(tokens.type_label).text_color(tone.head).child(label.to_uppercase()))
            .children(items.iter().map(|line| {
                if line.label == AGENT_MARK {
                    return h_flex()
                        .items_center()
                        .gap(rem(tokens.sp_8))
                        .min_w_0()
                        .child(div().text_color(tone.mark).child(Glyph::Console.at(tokens.icon_sm)))
                        .child(div().min_w_0().truncate().child(SharedString::from(line.text.clone())))
                        .children(line.aside.as_ref().map(|status| status_pill(SharedString::from(format!("peek-agent-{}", line.text)), status_word(status).to_owned(), status, tokens)))
                        .into_any_element();
                }
                v_flex()
                    .child(
                        h_flex()
                            .items_start()
                            .child(div().flex_shrink_0().text_color(tone.mark).child(format!("{mark} ")))
                            .child(div().flex_shrink_0().font_family(mono.clone()).text_color(tone.label).child(SharedString::from(line.label.clone())))
                            .child(div().flex_1().min_w_0().text_color(tone.text.unwrap_or(tokens.fg)).child(SharedString::from(format!(" {}", line.text)))),
                    )
                    .children(line.aside.clone().map(|aside| meta(aside, tokens).pl(rem(tokens.sp_20))))
                    .into_any_element()
            })),
    )
}
