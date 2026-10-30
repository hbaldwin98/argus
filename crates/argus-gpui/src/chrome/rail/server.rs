//! The server dot at the strip's foot: green while the daemon is connected, the attention colour
//! while it is being reached, hollow once it stopped, and ringed in the attention colour while the
//! daemon is another build than the window.
//!
//! The alarm the daemon raised (`Status::alarm`) says which build and what to run
//! (`argus server restart`); it is the ring's tooltip and the menu's first line, not a line of text
//! in the rail. A press opens the menu the old foot's other controls moved to: the connection in
//! words, the alarm, the console and the chime. The settings gear is the strip's own tile.

use argus_desktop_core::connection::page::Call;
use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants as _};
use gpui_kit::component::menu::DropdownMenu as _;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{div, prelude::*, Anchor, Context, SharedString, TestSupportExt as _};

use super::menus::Does;
use super::popup;
use super::row::Ctx;
use super::slice::Slice;
use super::RailView;
use crate::chrome::server::{label, words, Server};
use crate::connection::heard::Link;
use crate::kit::icons::Glyph;
use crate::kit::menu::item::MenuItem;
use crate::theme::scale::{pixels, rem};
use crate::theme::Tokens;

/// What the dot's menu offers: the connection's words, the alarm when there is one, then the console
/// and the chime, which the foot used to hold. A choice only names its call.
pub(super) fn items(server: &Server) -> Vec<MenuItem<Does>> {
    let mut items = vec![MenuItem::heading(words(&server.link))];
    if let Some(alarm) = &server.alarm {
        items.push(MenuItem::heading(alarm.clone()));
    }
    items.push(MenuItem::Rule);
    let mut console = MenuItem::choice(server.console_title.clone(), Does::Send(vec![Call::Console { gesture: argus_desktop_core::panes::console::ConsoleGesture::Toggle }]));
    if server.console.count > 0 {
        console = console.noted(server.console.count.to_string());
    }
    items.push(console);
    let chime = if server.chime { "Mute the chime when an agent needs you" } else { "Chime when an agent needs you" };
    items.push(MenuItem::choice(chime, Does::Send(vec![Call::SetChime { on: !server.chime }])));
    items
}

/// What hovering the dot says: the connection, and the alarm under it.
fn tooltip(server: &Server) -> String {
    match &server.alarm {
        Some(alarm) => format!("{}\n{alarm}", label(&server.link)),
        None => label(&server.link),
    }
}

impl RailView {
    pub(super) fn server_dot(&self, slice: &Slice, ctx: Ctx, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *tokens;
        let server = &slice.server;
        let named = label(&server.link);
        // A ring only while connected: the alarm is about the daemon that is there.
        let ringed = server.alarm.is_some() && matches!(server.link, Link::Connected { .. });
        let dot = div().id("foot-dot").test_support().aria_label(named.clone()).flex_shrink_0().size(rem(10.0));
        let dot = match &server.link {
            Link::Connected { .. } => dot.rounded_full().bg(tokens.ok).into_any_element(),
            Link::Connecting | Link::Reconnecting => dot.rounded_full().bg(tokens.attention).into_any_element(),
            Link::Stopped => dot.child(Glyph::Ring8.at(10.0).text_color(tokens.fg_faint)).into_any_element(),
        };
        // `box-shadow: 0 0 0 3px <strip>, 0 0 0 4.5px <attention>`: a gap of the strip's own colour, then the ring.
        let mark = div()
            .id("foot-ring")
            .test_support()
            .flex()
            .items_center()
            .justify_center()
            .size(rem(18.0))
            .rounded_full()
            .border_1()
            .border_color(if ringed { tokens.attention } else { gpui_kit::transparent_black() })
            .child(dot);
        let (rail, heard) = (cx.weak_entity(), self.heard.clone());
        let items = items(server);
        let variant = ButtonCustomVariant::new(cx).foreground(tokens.fg).hover(tokens.fill).active(tokens.bg_selected);
        let _ = ctx;
        Button::new("foot-status")
            .custom(variant)
            .size(rem(36.0))
            .rounded(pixels(8.0, cx.theme().font_size))
            .accessibility_label(SharedString::from(named))
            .tooltip(tooltip(server))
            .child(mark)
            .dropdown_menu_with_anchor(Anchor::BottomLeft, move |menu, window, cx| popup::fill(menu, &items, &rail, &heard, tokens, window, cx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kit::menu::item::lines;

    fn server(alarm: Option<&str>, chime: bool, consoles: usize) -> Server {
        Server {
            link: Link::Connected { host: None },
            alarm: alarm.map(str::to_owned),
            chime,
            console: crate::chrome::server::Console { shown: false, count: consoles },
            console_title: "Show the console".into(),
            settings_title: "Settings".into(),
        }
    }

    #[test]
    fn the_menu_says_the_connection_then_the_alarm_then_the_console_and_the_chime() {
        let said = lines(&items(&server(Some("argusd 0.42.2, window 0.42.4: run `argus server restart`"), true, 0)));
        assert_eq!(
            said,
            [
                "# argusd · local",
                "# argusd 0.42.2, window 0.42.4: run `argus server restart`",
                "---",
                "Show the console",
                "Mute the chime when an agent needs you"
            ]
        );
        assert_eq!(lines(&items(&server(None, false, 2)).into_iter().skip(2).collect::<Vec<_>>()), ["Show the console", "Chime when an agent needs you"]);
    }

    #[test]
    fn the_chime_entry_asks_for_the_opposite_of_what_was_heard() {
        for chime in [true, false] {
            let items = items(&server(None, chime, 0));
            let Does::Send(calls) = &items.last().unwrap().as_choice().unwrap().does else { panic!("a choice sends") };
            assert!(matches!(calls.as_slice(), [Call::SetChime { on }] if *on == !chime));
        }
    }

    #[test]
    fn the_tooltip_carries_the_alarm_under_the_connection() {
        assert_eq!(tooltip(&server(None, true, 0)), "argusd · local");
        assert_eq!(tooltip(&server(Some("run `argus server restart`"), true, 0)), "argusd · local\nrun `argus server restart`");
    }
}
