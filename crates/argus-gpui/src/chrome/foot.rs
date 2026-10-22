//! The sidebar's foot: the tallies that need a look, a daemon's alarm, and
//! the connection in the same shape in every state.
//!
//! One shape, so the foot does not rearrange while the daemon comes and
//! goes: only the dot and the words change. A remote daemon is named, so
//! what the rail lists is never taken for this machine's. Beside it are the
//! console chip and the chime switch, which send the page's calls; the settings
//! gear waits for a Settings dialog in this window.

use argus_desktop_core::connection::page::{Call, Rail};
use argus_desktop_core::panes::console::ConsoleGesture;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, px, Role};

use crate::connection::caller::Caller;
use crate::connection::heard::Link;
use crate::kit::icon_button::icon_button;
use crate::kit::icons::Glyph;
use crate::style::Typed as _;
use crate::theme::Tokens;

/// What the status row shows (SHELL-FOOT-3): the dot says connecting and
/// stopped on its own, so only a daemon that was there and is being reached
/// again earns words (`GPUI.md`, cross-cutting decision 1).
pub fn words(link: &Link) -> String {
    match link {
        Link::Connected { host } => format!("argusd · {}", host.as_deref().unwrap_or("local")),
        Link::Reconnecting => "argusd · reconnecting".to_owned(),
        Link::Connecting | Link::Stopped => "argusd".to_owned(),
    }
}

/// What the row and its dot are called to a reader that cannot see the
/// colour: the state in every case.
pub fn label(link: &Link) -> String {
    match link {
        Link::Connecting => "argusd · connecting".to_owned(),
        Link::Stopped => "argusd · stopped".to_owned(),
        other => words(other),
    }
}

/// What the console chip needs of the console: whether it is up, and how
/// many consoles the daemon holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Console {
    pub shown: bool,
    pub count: usize,
}

/// `.bar`: 8px over 16px of padding, 4px between rows, and the page's small type.
#[allow(clippy::too_many_arguments)]
pub fn foot(
    link: &Link,
    rail: &Rail,
    alarm: Option<&str>,
    chime: bool,
    console: Console,
    caller: &Caller,
    held: bool,
    tokens: &Tokens,
) -> impl IntoElement {
    let connected = matches!(link, Link::Connected { .. });
    let mut rows = v_flex()
        .flex_shrink_0()
        .gap(px(tokens.sp_4))
        .px(px(tokens.sp_16))
        .py(px(tokens.sp_8))
        .border_t_1()
        .border_color(tokens.line)
        .typed(tokens.type_small)
        .text_color(tokens.fg_muted);
    if connected && (rail.needs_you > 0 || rail.working > 0) {
        let mut tallies = h_flex().min_h(px(tokens.control_h_sm)).gap(px(tokens.sp_8)).items_center();
        if rail.needs_you > 0 {
            let text = format!("▲ {} need you", rail.needs_you);
            tallies = tallies.child(
                div()
                    .id("foot-need-you")
                    .role(Role::Status)
                    .test_support()
                    .aria_label(text.clone())
                    .weighted(tokens.weight_semibold)
                    .text_color(tokens.attention)
                    .child(text),
            );
        }
        if rail.working > 0 {
            let text = format!("⋮ {} working", rail.working);
            tallies = tallies.child(
                div()
                    .id("foot-working")
                    .role(Role::Status)
                    .test_support()
                    .aria_label(text.clone())
                    .text_color(tokens.accent)
                    .child(text),
            );
        }
        rows = rows.child(tallies);
    }
    if let Some(alarm) = alarm.filter(|_| connected) {
        rows = rows.child(
            div()
                .id("foot-alarm")
                .role(Role::Alert)
                .test_support()
                .aria_label(alarm.to_owned())
                .text_color(tokens.attention)
                .child(alarm.to_owned()),
        );
    }
    let said = words(link);
    let named = label(link);
    let console_chip = {
        let caller = caller.clone();
        let words = if console.shown { "Hide the console" } else { "Show the console" };
        icon_button("foot-console", words, console.shown, tokens)
            .px(px(tokens.sp_8))
            .w_auto()
            .gap(px(tokens.sp_4))
            .on_click(move |_, _, _| {
                if !held {
                    caller.call(Call::Console { gesture: ConsoleGesture::Toggle });
                }
            })
            .child(Glyph::Console.at(tokens.icon_sm))
            .when(console.count > 0, |chip| {
                chip.child(div().text_size(px(tokens.fs_xs)).child(console.count.to_string()))
            })
    };
    let chime_button = {
        let caller = caller.clone();
        let words =
            if chime { "Mute the chime when an agent needs you" } else { "Chime when an agent needs you" };
        icon_button("foot-chime", words, false, tokens)
            .on_click(move |_, _, _| {
                if !held {
                    caller.call(Call::SetChime { on: !chime });
                }
            })
            .child(if chime { Glyph::Bell } else { Glyph::BellOff }.at(tokens.icon_sm))
    };
    rows.child(
        h_flex()
            .min_h(px(tokens.control_h_sm))
            .gap(px(tokens.sp_8))
            .items_center()
            .child(
                h_flex()
                    .id("foot-status")
                    .role(Role::Status)
                    .test_support()
                    .aria_label(named.clone())
                    .flex_1()
                    .min_w_0()
                    .gap(px(tokens.sp_8))
                    .items_center()
                    .child(dot(link, named, tokens))
                    .child(div().flex_1().min_w_0().truncate().child(said)),
            )
            .child(console_chip)
            .child(chime_button),
    )
}

/// Green while connected, the attention colour while it tries, hollow once
/// stopped: `.dot`, 8px.
fn dot(link: &Link, named: String, tokens: &Tokens) -> impl IntoElement {
    let dot = div().id("foot-dot").test_support().aria_label(named).flex_shrink_0().size(px(tokens.dot_lg));
    match link {
        Link::Connected { .. } => dot.rounded_full().bg(tokens.ok).into_any_element(),
        Link::Connecting | Link::Reconnecting => dot.rounded_full().bg(tokens.attention).into_any_element(),
        Link::Stopped => dot.child(Glyph::Ring8.at(tokens.dot_lg).text_color(tokens.fg_faint)).into_any_element(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_status_row_names_the_host_the_daemon_is_reached_on() {
        assert_eq!(words(&Link::Connected { host: None }), "argusd · local");
        assert_eq!(words(&Link::Connected { host: Some("dev-box".into()) }), "argusd · dev-box");
        assert_eq!(words(&Link::Reconnecting), "argusd · reconnecting");
        assert_eq!(words(&Link::Connecting), "argusd");
        assert_eq!(words(&Link::Stopped), "argusd");
    }

    #[test]
    fn the_state_is_named_to_a_reader_that_cannot_see_the_dot() {
        assert_eq!(label(&Link::Connecting), "argusd · connecting");
        assert_eq!(label(&Link::Stopped), "argusd · stopped");
        assert_eq!(label(&Link::Reconnecting), "argusd · reconnecting");
        assert_eq!(label(&Link::Connected { host: None }), "argusd · local");
    }
}
