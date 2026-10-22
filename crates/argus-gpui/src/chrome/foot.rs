//! The sidebar's foot: the tallies that need a look, a daemon's alarm, and
//! the connection in the same shape in every state.
//!
//! One shape, so the foot does not rearrange while the daemon comes and
//! goes: only the dot and the words change. A remote daemon is named, so
//! what the rail lists is never taken for this machine's. Beside it are the
//! console chip, the chime switch and the settings gear, each with its words (and keys) as hover
//! text. A narrow rail keeps the dot and the three buttons, stacked.

use argus_desktop_core::connection::page::Call;
use argus_desktop_core::keys::keymap::Action;
use argus_desktop_core::panes::console::ConsoleGesture;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, Role};

use crate::connection::caller::Caller;
use crate::connection::heard::Link;
use crate::keys::over::run_over;
use crate::kit::icon_button::icon_button;
use crate::kit::icons::Glyph;
use crate::kit::tip::tip;
use crate::style::Typed as _;
use crate::theme::scale::rem;
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

/// Everything the foot says: what the rail and the status hold of it, and the hover text of its
/// buttons with the keys the keymap names for them.
#[derive(Debug, Clone, PartialEq)]
pub struct Foot {
    pub link: Link,
    pub needs_you: usize,
    pub working: usize,
    pub alarm: Option<String>,
    pub chime: bool,
    pub console: Console,
    /// `Hide the console (\`)`: the chip's words, and its keys when the keymap has been heard.
    pub console_title: String,
    pub settings_title: String,
}

/// `.bar`: 8px over 16px of padding, 4px between rows, and the page's small type.
pub fn foot(foot: &Foot, caller: &Caller, held: bool, icons: bool, tokens: &Tokens) -> impl IntoElement {
    let Foot { link, needs_you, working, alarm, chime, console, .. } = foot;
    let (needs_you, working, chime) = (*needs_you, *working, *chime);
    let connected = matches!(link, Link::Connected { .. });
    let mut rows = v_flex()
        .flex_shrink_0()
        .gap(rem(tokens.sp_4))
        .px(rem(if icons { 0.0 } else { tokens.sp_16 }))
        .py(rem(tokens.sp_8))
        .when(icons, |rows| rows.items_center())
        .border_t_1()
        .border_color(tokens.line)
        .typed(tokens.type_small)
        .text_color(tokens.fg_muted);
    // A narrow rail has no room for words: the tallies and the alarm are put away.
    if !icons && connected && (needs_you > 0 || working > 0) {
        let mut tallies = h_flex().min_h(rem(tokens.control_h_sm)).gap(rem(tokens.sp_8)).items_center();
        if needs_you > 0 {
            let text = format!("▲ {needs_you} need you");
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
        if working > 0 {
            let text = format!("⋮ {working} working");
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
    if let Some(alarm) = alarm.as_deref().filter(|_| connected && !icons) {
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
    let glyph = if icons { tokens.icon_glyph } else { tokens.icon_sm };
    let console_chip = {
        let caller = caller.clone();
        let words = if console.shown { "Hide the console" } else { "Show the console" };
        icon_button("foot-console", words, console.shown, tokens)
            .tooltip(tip(foot.console_title.clone()))
            .when(!icons, |chip| chip.px(rem(tokens.sp_8)).w_auto().gap(rem(tokens.sp_4)))
            .relative()
            .on_click(move |_, _, _| {
                if !held {
                    caller.call(Call::Console { gesture: ConsoleGesture::Toggle });
                }
            })
            .child(Glyph::Console.at(glyph))
            .when(console.count > 0 && !icons, |chip| {
                chip.child(div().text_size(rem(tokens.fs_xs)).child(console.count.to_string()))
            })
            // `.bar .icount`: a badge on the corner of the tile.
            .when(console.count > 0 && icons, |chip| {
                chip.child(
                    div()
                        .id("foot-console-count")
                        .test_support()
                        .absolute()
                        .top(rem(-tokens.sp_4))
                        .right(rem(-tokens.sp_4))
                        .min_w(rem(tokens.sp_16))
                        .px(rem(tokens.sp_4))
                        .border_1()
                        .border_color(tokens.line)
                        .rounded(rem(tokens.r_pill))
                        .bg(tokens.bg_raised)
                        .text_color(tokens.fg_muted)
                        .text_size(rem(tokens.fs_xs))
                        .line_height(rem(tokens.sp_16))
                        .text_center()
                        .child(console.count.to_string()),
                )
            })
    };
    let chime_button = {
        let caller = caller.clone();
        let words =
            if chime { "Mute the chime when an agent needs you" } else { "Chime when an agent needs you" };
        icon_button("foot-chime", words, false, tokens)
            .tooltip(tip(words))
            .on_click(move |_, _, _| {
                if !held {
                    caller.call(Call::SetChime { on: !chime });
                }
            })
            .child(if chime { Glyph::Bell } else { Glyph::BellOff }.at(glyph))
    };
    // Settings is the window's, not the daemon's: it opens while the daemon is gone.
    let settings = icon_button("foot-settings", "Settings", false, tokens)
        .tooltip(tip(foot.settings_title.clone()))
        .on_click(|_, window, cx| {
            run_over(Action::Settings, window, cx);
        })
        .child(Glyph::Gear.at(glyph));
    let status = h_flex()
        .id("foot-status")
        .role(Role::Status)
        .test_support()
        .aria_label(named.clone())
        .gap(rem(tokens.sp_8))
        .items_center()
        .child(dot(link, named, tokens))
        .when(!icons, |status| status.flex_1().min_w_0().child(div().flex_1().min_w_0().truncate().child(said)));
    rows.child(
        div()
            .flex()
            .when(icons, |row| row.flex_col().items_center().gap(rem(tokens.sp_4)))
            .when(!icons, |row| row.min_h(rem(tokens.control_h_sm)).gap(rem(tokens.sp_8)).items_center())
            .child(status)
            .child(console_chip)
            .child(chime_button)
            .child(settings),
    )
}

/// Green while connected, the attention colour while it tries, hollow once
/// stopped: `.dot`, 8px.
fn dot(link: &Link, named: String, tokens: &Tokens) -> impl IntoElement {
    let dot = div().id("foot-dot").test_support().aria_label(named).flex_shrink_0().size(rem(tokens.dot_lg));
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
