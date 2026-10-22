//! The foot of the side column: Files, the folder in the file manager, `New pane`, and at the other
//! end the removal in danger (`.cs-foot`). Each is a key the table answers to as well.
//!
//! `New pane` toggles the start menu for the checkout, asked of the session's cursor with the
//! table as its source; the menu opens upward since the button sits at the column's foot, and the
//! button is lit while it is open.

use std::cell::Cell;
use std::rc::Rc;

use argus_desktop_core::checkouts::side::Foot;
use argus_desktop_core::checkouts::ways::Way;
use argus_desktop_core::chrome::cursor::StartFrom;
use argus_desktop_core::connection::page::Call;
use argus_desktop_core::keys::keymap::{Action, Place};
use gpui_kit::component::h_flex;
use gpui_kit::{deferred, div, prelude::*, App, SharedString, TestSupportExt as _};

use super::super::table::Ctx;
use crate::checkouts::act;
use crate::keys::caps;
use crate::kit::button::{button, outlined, Look};
use crate::kit::icons::Glyph;
use crate::kit::keycap::keycap;
use crate::kit::start::{menu, Start};
use crate::kit::tip::tip;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// A quiet small button: no edge, muted, with an icon and perhaps the key that does the same.
fn quiet(id: &'static str, words: &str, icon: Glyph, key: Option<&str>, tokens: &Tokens) -> crate::kit::icon_button::Node {
    let mut button = button(id, words.to_owned(), Look::Quiet, true, tokens).child(icon.at(tokens.icon_sm)).child(SharedString::from(words.to_owned()));
    if let Some(key) = key {
        button = button.child(keycap(key.to_owned(), SharedString::from(tokens.font_mono), tokens));
    }
    button
}

#[allow(clippy::too_many_arguments)]
pub fn foot(foot: &Foot, name: &str, key: &str, ctx: &Ctx, hover: &Rc<Cell<bool>>, tokens: &Tokens, cx: &App) -> impl IntoElement {
    let heard = ctx.heard.read(cx);
    let (held, start, keymap, templates) = (heard.held(), heard.rail.cursor.start, heard.keymap.clone(), heard.rail.templates.clone());
    let hot = foot.new_pane.is_some_and(|checkout| start.is_some_and(|open| open.checkout == checkout && open.from == StartFrom::Table));
    let (caller, here) = (ctx.caller.clone(), ctx.heard.clone());
    let press = |way: Way| {
        let (caller, heard, key) = (caller.clone(), here.clone(), key.to_owned());
        move |_: &gpui_kit::ClickEvent, window: &mut gpui_kit::Window, cx: &mut App| act::way(&way, &key, &caller, &heard, window, cx)
    };
    let start_menu = foot.new_pane.filter(|_| hot).map(|checkout| {
        let caps_of = |action| caps::presses(&keymap, action, Place::Start).into_iter().next().unwrap_or_default();
        let (out, chose) = (ctx.caller.clone(), ctx.caller.clone());
        let plus = hover.clone();
        let menu = menu(
            Start {
                checkout,
                label: format!("Start on {name}"),
                at: start.map_or(0, |open| open.at),
                templates: &templates,
                shell_keys: caps_of(Action::StartShell),
                agent_keys: caps_of(Action::StartAgent),
            },
            // A press anywhere else shuts it; the button that opened it toggles it itself.
            move |_, _| {
                if !plus.get() && !held {
                    out.call(Call::StartKey { action: Action::CloseLayer });
                }
            },
            move |template, _, _| {
                if !held {
                    chose.call(Call::StartOn { checkout, template });
                }
            },
            tokens,
        );
        deferred(menu.absolute().bottom(rem(tokens.sp_32)).left_0())
    });
    let (toggler, plus) = (ctx.caller.clone(), hover.clone());
    h_flex()
        .id("side-foot")
        .test_support()
        .flex_wrap()
        .items_center()
        .gap(rem(tokens.sp_4))
        .border_t_1()
        .border_color(tokens.line)
        .pt(rem(tokens.sp_12))
        .when(foot.files, |foot| foot.child(quiet("side-files", "Files", Glyph::Folder, Some("E"), tokens).on_click(press(Way::Files))))
        .when_some(foot.reveal, |foot_row, words| foot_row.child(quiet("side-reveal", words, Glyph::External, None, tokens).on_click(press(Way::Reveal))))
        .when_some(foot.new_pane, move |foot_row, checkout| {
            foot_row.child(
                div().relative().child(
                    quiet("side-new-pane", "New pane", Glyph::Plus, None, tokens)
                        .aria_expanded(hot)
                        .tooltip(tip("Start a shell or an agent"))
                        .when(hot, |button| button.bg(tokens.fill).text_color(tokens.accent))
                        .on_hover(move |hovered, _, _| plus.set(*hovered))
                        .on_click(move |_, _, cx| {
                            cx.stop_propagation();
                            if !held {
                                toggler.call(Call::ToggleStart { checkout, from: StartFrom::Table });
                            }
                        }),
                )
                .children(start_menu),
            )
        })
        .child(div().flex_1())
        .when_some(foot.remove, |foot_row, words| {
            foot_row.child(outlined("side-remove", words, tokens.danger, true, tokens).child(words).child(keycap("D", SharedString::from(tokens.font_mono), tokens)).on_click(press(Way::Delete)))
        })
}
