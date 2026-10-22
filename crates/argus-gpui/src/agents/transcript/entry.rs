//! One thing said or done in a session, as the sessions preview and the full reader both draw it:
//! a person's turn or the agent's reply with its caption, or a tool call told as a verb, a
//! summary and whether it failed (TRANSCRIPT-3).
//!
//! A line of a kind this window does not know (a newer daemon's) draws nothing; core already
//! leaves it out, and the guard here keeps one that slipped through from drawing as an empty
//! block (TRANSCRIPT-15).

use argus_desktop_core::agents::said::{tool_verb, ToolIcon};
use argus_desktop_core::connection::page::TranscriptLine;
use argus_desktop_core::system::clock::Clock;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, AnyElement, SharedString, TestSupportExt as _};

use crate::kit::icons::Glyph;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// How find has marked a line: not at all, as one of its matches, or the match it is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    None,
    Matched,
    Found,
}

fn glyph(icon: ToolIcon) -> Glyph {
    match icon {
        ToolIcon::Read => Glyph::ToolRead,
        ToolIcon::Search => Glyph::ToolSearch,
        ToolIcon::Edit => Glyph::ToolEdit,
        ToolIcon::Run => Glyph::ToolRun,
    }
}

/// Whether `line` is of a kind that draws.
pub fn draws(line: &TranscriptLine) -> bool {
    matches!(line.kind, "user" | "agent" | "tool")
}

/// The line named `id`, or nothing for a kind that is not drawn. `mark` is find's outline: a
/// hairline for a match, the accent edge for the one found: a border kept transparent otherwise, since the kit paints a shadow filled behind a box that has none of its own.
pub fn entry(id: impl Into<gpui_kit::ElementId>, line: &TranscriptLine, mark: Mark, clock: &Clock, tokens: &Tokens) -> Option<AnyElement> {
    if !draws(line) {
        return None;
    }
    let outline = match mark {
        Mark::None => None,
        Mark::Matched => Some(tokens.line_strong),
        Mark::Found => Some(tokens.accent),
    };
    let mono = SharedString::from(tokens.font_mono);
    if line.kind == "tool" {
        let verb = tool_verb(&line.tool);
        let said = verb.map_or_else(|| line.tool.clone(), |(_, words)| words.to_owned());
        let colour = if line.failed { tokens.danger } else { tokens.fg_muted };
        let label = SharedString::from(format!("{said} {}{}", line.text, if line.failed { " failed" } else { "" }));
        return Some(
            h_flex()
                .id(id)
                .test_support()
                .aria_label(label)
                .flex_shrink_0()
                .min_w_0()
                .gap(rem(tokens.sp_8))
                .items_center()
                .rounded(rem(tokens.r_sm))
                .text_color(colour)
                .border_1()
                .border_color(outline.unwrap_or_else(gpui_kit::transparent_black))
                .children(verb.map(|(icon, _)| div().flex_shrink_0().child(glyph(icon).at(tokens.icon_sm))))
                .child(div().flex_shrink_0().font_family(SharedString::from(tokens.font_ui)).child(SharedString::from(said)))
                .child(div().min_w_0().truncate().font_family(mono).child(SharedString::from(line.text.clone())))
                .when(line.failed, |row| row.child(div().flex_shrink_0().text_color(tokens.danger).child("failed")))
                .into_any_element(),
        );
    }
    let user = line.kind == "user";
    let at = clock.hhmm(line.at.as_deref());
    let caption = format!("{}{}", if user { "You" } else { "Agent" }, at.map(|at| format!(" \u{b7} {at}")).unwrap_or_default());
    Some(
        v_flex()
            .id(id)
            .test_support()
            .aria_label(SharedString::from(line.text.clone()))
            .flex_shrink_0()
            .gap(rem(tokens.sp_2))
            .rounded(rem(tokens.r_sm))
            .border_1()
            .border_color(outline.unwrap_or_else(gpui_kit::transparent_black))
            .child(div().typed(tokens.type_small).text_color(tokens.fg_muted).child(SharedString::from(caption)))
            .child(
                div()
                    .font_family(SharedString::from(tokens.font_ui))
                    .text_color(tokens.fg)
                    // The person's turn is a quiet block with its own label: the accent is for what is selected.
                    .when(user, |text| {
                        text.px(rem(tokens.sp_12)).py(rem(tokens.sp_8)).rounded(rem(tokens.r_md)).border_1().border_color(tokens.line).bg(tokens.bg_raised)
                    })
                    .child(SharedString::from(line.text.clone())),
            )
            .into_any_element(),
    )
}
