//! A cell's tab strip (SHELL-BROWSER-6): the browser's tabs, a plus, who has it, and the person's
//! way to take it or hand it back.
//!
//! The strip is drawn again whenever the browser's page is, and its tabs scroll in a handle the
//! cell keeps, so an update no longer resets where they were scrolled to (the page's strip does).
//! Nothing here changes what is drawn: each press is sent, and the daemon's page says what came
//! of it.

use argus_desktop_core::browser::status::status;
use argus_desktop_core::browser::PageBrowser;
use argus_desktop_core::connection::page::Call;
use argus_protocol::{BrowserInput, BrowserTab};
use gpui_kit::component::h_flex;
use gpui_kit::{div, prelude::*, MouseButton, ScrollHandle, SharedString, TestSupportExt as _};

use super::bar::text_button;
use super::chip::chip;
use crate::connection::caller::Caller;
use crate::kit::icon_button::{icon_button, Node};
use crate::kit::icons::Glyph;
use crate::kit::strip;
use crate::kit::tip::tip;
use crate::theme::scale::rem;
use crate::theme::Tokens;

pub const NEW_TAB: &str = "New tab";
pub const CLOSE_TAB: &str = "Close this tab";
pub const FOLLOW: &str = "Follow agent";
pub const FOLLOW_TIP: &str = "Show the tab the agent works in, as it moves";
pub const TAKE: &str = "Take control";
pub const TAKE_TIP: &str = "Hold the browser: the agent’s clicks and typing are refused until you hand it back";
pub const HAND_BACK: &str = "Hand back";
pub const HAND_BACK_TIP: &str = "Let the agent drive again";

/// A tab's words: its title, else its url, else `New tab`.
pub fn tab_name(tab: &BrowserTab) -> &str {
    if !tab.title.is_empty() {
        &tab.title
    } else if !tab.url.is_empty() {
        &tab.url
    } else {
        NEW_TAB
    }
}

/// `.vpanel-tabs.btabs`.
pub fn strip(browser: &PageBrowser, scroll: &ScrollHandle, caller: &Caller, tokens: &Tokens) -> impl IntoElement {
    let pane = browser.pane;
    let send = move |caller: &Caller, input: BrowserInput| caller.call(Call::BrowserInput { pane, input });
    let tabs = browser.tabs.iter().map(|tab| tab_chip(pane, tab, browser.shown.as_deref() == Some(tab.id.as_str()), caller, tokens));
    let plus = {
        let caller = caller.clone();
        icon_button(SharedString::from(format!("browser-new-tab-{pane}")), NEW_TAB, false, tokens)
            .tooltip(tip(NEW_TAB))
            .child(Glyph::Plus.at(tokens.icon_xs))
            .on_click(move |_, _, _| send(&caller, BrowserInput::NewTab))
    };
    let follow = (!browser.following).then(|| {
        let caller = caller.clone();
        compact(text_button(format!("browser-follow-{pane}"), FOLLOW, FOLLOW_TIP, tokens, move |_, _| send(&caller, BrowserInput::FollowAgent)), tokens)
    });
    let hold = {
        let caller = caller.clone();
        if browser.held {
            compact(text_button(format!("browser-hold-{pane}"), HAND_BACK, HAND_BACK_TIP, tokens, move |_, _| send(&caller, BrowserInput::Hold { held: false })), tokens)
        } else {
            compact(text_button(format!("browser-hold-{pane}"), TAKE, TAKE_TIP, tokens, move |_, _| send(&caller, BrowserInput::Hold { held: true })), tokens)
        }
    };
    strip::bar(tokens)
        .id(SharedString::from(format!("browser-tabs-{pane}")))
        .test_support()
        .aria_label("Tabs")
        .child(
            h_flex()
                .id(SharedString::from(format!("browser-tabs-scroll-{pane}")))
                .test_support()
                .flex_1()
                .min_w_0()
                .gap(rem(tokens.sp_4))
                .items_center()
                .overflow_x_scroll()
                .track_scroll(scroll)
                .children(tabs),
        )
        .child(plus)
        .children(status(browser).map(|status| chip(status, tokens)))
        .children(follow)
        .child(hold)
}

/// `.btabs .btn`: the strip's buttons keep 8px either side of the words.
fn compact(button: Node, tokens: &Tokens) -> Node {
    button.px(rem(tokens.sp_8))
}

/// One tab: picked by a press, closed by its ✕ or a middle click.
fn tab_chip(pane: u64, tab: &BrowserTab, on: bool, caller: &Caller, tokens: &Tokens) -> impl IntoElement {
    let name = SharedString::from(tab_name(tab).to_owned());
    let (pick, middle, x) = (caller.clone(), caller.clone(), caller.clone());
    let (show, gone_too, gone_x) = (tab.id.clone(), tab.id.clone(), tab.id.clone());
    let id = SharedString::from(format!("browser-tab-{pane}-{}", tab.id));
    let tooltip = SharedString::from(tab.url.clone());
    let group = SharedString::from(format!("browser-tab-group-{pane}-{}", tab.id));
    let (spacer, shown_group) = (rem(tokens.sp_8), group.clone());
    let (open_w, open_mr) = (tokens.control_h_sm, tokens.sp_4);
    let close_x = strip::close_x(SharedString::from(format!("browser-tab-close-{pane}-{}", tab.id)), format!("Close {name}"), tokens);
    // A tab that is not shown has no close until the pointer is on it, and is as wide as its words:
    // widths change on hover and nothing is shown or hidden, which GPUI cannot do between a frame's passes.
    strip::tab(id, on, true, name.clone(), tokens)
        .group(group.clone())
        .tooltip(tip(if tooltip.is_empty() { name.clone() } else { tooltip }))
        .child(strip::title(name.clone()).flex_1())
        .when(!on, move |tab| tab.child(div().flex_none().w(spacer).group_hover(shown_group, |style| style.w(rem(0.0)))))
        .child(
            close_x
                .when(!on, move |x| x.w(rem(0.0)).mr(rem(0.0)).overflow_hidden().group_hover(group, move |style| style.w(rem(open_w)).mr(rem(open_mr))))
                .tooltip(tip(CLOSE_TAB))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    x.call(Call::BrowserInput { pane, input: BrowserInput::CloseTab { id: gone_x.clone() } });
                }),
        )
        .on_click(move |_, _, _| pick.call(Call::BrowserInput { pane, input: BrowserInput::ShowTab { id: show.clone() } }))
        .on_mouse_up(MouseButton::Middle, move |_, _, _| middle.call(Call::BrowserInput { pane, input: BrowserInput::CloseTab { id: gone_too.clone() } }))
}
