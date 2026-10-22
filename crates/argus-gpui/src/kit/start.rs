//! A checkout's start menu as it is drawn, wherever it opens: a shell, a rule, then an agent per
//! template, the highlighted one the session's cursor says (`chrome.ts`, `startMenu`).
//!
//! It is drawn in the page's own place rather than as the kit's popup, because the cursor that
//! moves through it is the session's and its keys are their own layer (`keys`, `Layer::Start`):
//! the kit's menu keeps its highlight to itself. The rail opens it under a row and the Checkouts
//! stage above its `New pane` button; each positions what this returns and says what a press
//! outside and a choice do.

use argus_desktop_core::chrome::cursor::starts;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, App, Role, SharedString, TestSupportExt as _, Window};

use crate::kit::icon_button::Node;
use crate::kit::keycap::keycap;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// What the menu lists and says.
pub struct Start<'a> {
    /// The checkout it is of, which names its element.
    pub checkout: u64,
    /// What the menu is called, for a reader that cannot see where it opened.
    pub label: String,
    /// The highlighted item.
    pub at: usize,
    pub templates: &'a [String],
    /// The keys that start a shell and the default agent, as the keymap names them.
    pub shell_keys: Vec<String>,
    pub agent_keys: Vec<String>,
}

/// The menu, with no position: a card under the page's `.menu`. `out` runs on a press anywhere
/// else, `chose` with the template of the item pressed (`None` for the shell), and the caller
/// positions it and puts it over what is under it. A click on an item does not pick the row.
pub fn menu(
    start: Start,
    out: impl Fn(&mut Window, &mut App) + 'static,
    chose: impl Fn(Option<String>, &mut Window, &mut App) + Clone + 'static,
    tokens: &Tokens,
) -> Node {
    let Start { checkout, label, at, templates, shell_keys, agent_keys } = start;
    let mono = SharedString::from(tokens.font_mono);
    let (fill, tokens) = (tokens.fill, *tokens);
    v_flex()
        .id(format!("start-menu-{checkout}"))
        .test_support()
        .role(Role::Menu)
        .aria_label(label)
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
        .on_mouse_down_out(move |_, window, cx| out(window, cx))
        .children(starts(templates).into_iter().enumerate().flat_map(|(i, item)| {
            let on = i == at;
            let template = item.agent.clone();
            let chose = chose.clone();
            let keys = match i {
                0 => shell_keys.clone(),
                1 => agent_keys.clone(),
                _ => Vec::new(),
            };
            let rule = (i == 1).then(|| div().h(rem(tokens.hairline)).mx(rem(tokens.sp_2)).my(rem(tokens.sp_4)).bg(tokens.line).into_any_element());
            let row = h_flex()
                .id(format!("start-{checkout}-{i}"))
                .role(Role::MenuItem)
                .test_support()
                .h(rem(tokens.control_h))
                .flex_shrink_0()
                .items_center()
                .rounded(rem(tokens.r_sm))
                .cursor_pointer()
                .w_full()
                .px(rem(tokens.sp_8))
                .gap(rem(tokens.sp_8))
                .aria_selected(on)
                .when(on, |row| row.bg(tokens.bg_selected))
                .hover(move |style| style.bg(fill))
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    chose(template.clone(), window, cx);
                })
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
        }))
}
