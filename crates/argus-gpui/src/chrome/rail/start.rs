//! A checkout's + menu, open under its row: a shell, then an agent per template, the highlighted
//! one the session's cursor says (`chrome.ts`, `startMenu`).
//!
//! It is drawn in the row, as the page draws it, rather than as the kit's popup, because the cursor
//! that moves through it is the session's and its keys are their own layer (`keys`, `Layer::Start`):
//! the kit's menu keeps its highlight to itself. A click on an item starts it without picking the
//! row; a press anywhere else shuts the menu.

use argus_desktop_core::chrome::cursor::starts;
use argus_desktop_core::connection::page::{Call, Checkout, Rail};
use argus_desktop_core::keys::keymap::{Action, Place};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{deferred, div, prelude::*, Context, Role, SharedString};

use super::row::line;
use super::RailView;
use crate::keys::caps;
use crate::kit::keycap::keycap;
use crate::theme::scale::rem;
use crate::theme::Tokens;


impl RailView {
    pub(super) fn start_menu(&self, rail: &Rail, repository: &str, checkout: &Checkout, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let (id, held) = (checkout.id, self.heard.read(cx).held());
        let at = rail.cursor.start.map_or(0, |menu| menu.at);
        let mono = SharedString::from(tokens.font_mono);
        let keymap = &self.heard.read(cx).keymap;
        let caps_of = |action| caps::presses(keymap, action, Place::Start).into_iter().next().unwrap_or_default();
        let (shell_keys, agent_keys) = (caps_of(Action::StartShell), caps_of(Action::StartAgent));
        let (fill, tokens) = (tokens.fill, *tokens);
        let items = starts(&rail.templates);
        let menu = v_flex()
            .id(format!("start-menu-{id}"))
            .test_support()
            .role(Role::Menu)
            .aria_label(format!("Start on {repository} {}", checkout.name))
            .absolute()
            .top(rem(tokens.menu_top))
            .right(rem(tokens.sp_8))
            .w(rem(tokens.menu_w))
            .occlude()
            .p(rem(tokens.sp_4))
            .bg(tokens.bg_surface)
            .border_1()
            .border_color(tokens.line_strong)
            .rounded(rem(tokens.r_md))
            .shadow(tokens.shadow_pop())
            .text_size(rem(tokens.fs_md))
            .text_color(tokens.fg)
            // A press anywhere else shuts it; the + that opened it toggles it itself.
            .on_mouse_down_out(cx.listener(move |this, _, _, _| {
                if !this.plus_hover.get() {
                    this.ask(held, Call::StartKey { action: Action::CloseLayer });
                }
            }))
            .children(items.into_iter().enumerate().flat_map(|(i, item)| {
                let on = i == at;
                let template = item.agent.clone();
                let keys = match i {
                    0 => shell_keys.clone(),
                    1 => agent_keys.clone(),
                    _ => Vec::new(),
                };
                let rule = (i == 1).then(|| div().h(rem(tokens.hairline)).mx(rem(tokens.sp_2)).my(rem(tokens.sp_4)).bg(tokens.line).into_any_element());
                let row = line(format!("start-{id}-{i}"), tokens.control_h, &tokens)
                    .role(Role::MenuItem)
                    .w_full()
                    .px(rem(tokens.sp_8))
                    .gap(rem(tokens.sp_8))
                    .aria_selected(on)
                    .when(on, |row| row.bg(tokens.bg_selected))
                    .hover(move |style| style.bg(fill))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.ask(held, Call::StartOn { checkout: id, template: template.clone() });
                    }))
                    .child(
                        div()
                            .w(rem(tokens.fs_sm))
                            .flex_shrink_0()
                            .font_family(mono.clone())
                            .text_color(if item.agent.is_some() { tokens.accent } else { tokens.fg_faint })
                            .child(if item.agent.is_some() { "\u{25c6}" } else { "\u{203a}" }),
                    )
                    .child(div().flex_1().min_w_0().truncate().child(item.label))
                    .child(h_flex().gap(rem(tokens.sp_4)).children(keys.into_iter().map(|key| keycap(key, mono.clone(), &tokens))))
                    .into_any_element();
                rule.into_iter().chain([row])
            }));
        deferred(menu)
    }
}
