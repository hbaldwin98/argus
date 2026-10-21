//! The sidebar's foot: the tallies that need a look, a daemon's alarm, and
//! the connection in the same shape in every state.
//!
//! One shape, so the foot does not rearrange while the daemon comes and
//! goes: only the dot and the words change. A remote daemon is named, so
//! what the rail lists is never taken for this machine's. The console chip,
//! chime switch and settings gear (SHELL-FOOT-4 to 6) are a later step's.

use argus_desktop_core::connection::page::Rail;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, px, Hsla, Role};

use crate::connection::heard::Link;
use crate::theme::Ledger;

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

pub fn foot(link: &Link, rail: &Rail, alarm: Option<&str>, ledger: &Ledger) -> impl IntoElement {
    let connected = matches!(link, Link::Connected { .. });
    let mut rows = v_flex().flex_shrink_0().gap_1().px_3().py_2().border_t_1().border_color(ledger.line).text_xs();
    if connected && (rail.needs_you > 0 || rail.working > 0) {
        let mut tallies = h_flex().gap_3();
        if rail.needs_you > 0 {
            let text = format!("▲ {} need you", rail.needs_you);
            tallies = tallies.child(
                div()
                    .id("foot-need-you")
                    .role(Role::Status)
                    .test_support()
                    .aria_label(text.clone())
                    .text_color(ledger.warning)
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
                    .text_color(ledger.accent)
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
                .text_color(ledger.warning)
                .child(alarm.to_owned()),
        );
    }
    let said = words(link);
    let named = label(link);
    rows.child(
        h_flex()
            .id("foot-status")
            .role(Role::Status)
            .test_support()
            .aria_label(named.clone())
            .gap_2()
            .items_center()
            .text_color(ledger.faint)
            .child(dot(link, named, ledger))
            .child(div().flex_1().min_w_0().truncate().child(said)),
    )
}

/// Green while connected, the attention colour while it tries, hollow once
/// stopped.
fn dot(link: &Link, named: String, ledger: &Ledger) -> impl IntoElement {
    let (fill, ring): (Option<Hsla>, Hsla) = match link {
        Link::Connected { .. } => (Some(ledger.success), ledger.success),
        Link::Connecting | Link::Reconnecting => (Some(ledger.warning), ledger.warning),
        Link::Stopped => (None, ledger.faint),
    };
    let dot = div()
        .id("foot-dot")
        .test_support()
        .aria_label(named)
        .size(px(7.0)).flex_shrink_0().rounded_full().border_1().border_color(ring);
    match fill {
        Some(fill) => dot.bg(fill),
        None => dot,
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
