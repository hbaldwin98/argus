//! One browser's tile on the Browsers stage (SHELL-BROWSER-2): the agent that owns it, where its
//! page is, and its latest picture a little smaller than its cell shows it.
//!
//! A tile says nothing to the daemon but where the person looked: its press asks the session to
//! put that browser's pane on screen in the Workspace, exactly as a click on its rail row does,
//! and it changes nothing here until the session's tree says it did (SHELL-BROWSER-4). The chip in
//! its head says who has the browser, in core's words (SHELL-BROWSER-9).

use argus_desktop_core::browser::status::status;
use argus_desktop_core::browser::PageBrowser;
use argus_desktop_core::connection::page::Call;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, relative, AnyView, Role, SharedString, TestSupportExt as _};

use super::chip::chip;
use crate::connection::caller::Caller;
use crate::kit::tip::tip;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// The words that say where a page is: its title when it has one that is not its address, and the
/// address, `about:blank` when empty.
pub fn where_of(browser: &PageBrowser) -> (Option<&str>, &str) {
    let title = (!browser.title.is_empty() && browser.title != browser.url).then_some(browser.title.as_str());
    (title, if browser.url.is_empty() { "about:blank" } else { browser.url.as_str() })
}

/// `.btile`, its picture being the view `picture` draws.
pub fn tile(browser: &PageBrowser, picture: AnyView, caller: &Caller, held: bool, tokens: &Tokens) -> impl IntoElement {
    let (pane, caller) = (browser.pane, caller.clone());
    let words = SharedString::from(format!("Open {}'s browser", browser.agent));
    let (title, url) = where_of(browser);
    let (line, strong, fill, muted) = (tokens.line, tokens.line_strong, tokens.bg_surface, tokens.fg_muted);
    v_flex()
        .id(SharedString::from(format!("browser-tile-{pane}")))
        .test_support()
        .role(Role::Button)
        .aria_label(words.clone())
        .tooltip(tip(words))
        .min_w_0()
        .overflow_hidden()
        .border_1()
        .border_color(line)
        .rounded(rem(tokens.r_md))
        .bg(fill)
        .text_color(tokens.fg)
        .typed(tokens.type_body)
        .cursor_pointer()
        .hover(move |style| style.border_color(strong))
        .on_click(move |_, _, _| {
            if !held {
                caller.call(Call::ShowBrowsers { pane: Some(pane) });
            }
        })
        .child(
            v_flex()
                .min_w_0()
                .gap(rem(tokens.sp_2))
                .px(rem(tokens.sp_12))
                .py(rem(tokens.sp_8))
                .border_b_1()
                .border_color(line)
                .child(
                    h_flex()
                        .min_w_0()
                        .items_center()
                        .gap(rem(tokens.sp_8))
                        .child(div().min_w_0().truncate().weighted(tokens.weight_semibold).child(SharedString::from(browser.agent.clone())))
                        .children(chip(format!("browser-tile-{pane}-status"), status(browser), tokens)),
                )
                .child(
                    h_flex()
                        .min_w_0()
                        .gap(rem(tokens.sp_8))
                        .text_size(rem(tokens.fs_sm))
                        .children(title.map(|title| div().flex_none().max_w(relative(0.5)).truncate().child(SharedString::from(title.to_owned()))))
                        .child(div().min_w_0().truncate().font_family(SharedString::from(tokens.font_mono)).text_color(muted).child(SharedString::from(url.to_owned()))),
                ),
        )
        .child(div().w_full().aspect_ratio(tokens.browser_tile_ratio).bg(tokens.bg_chrome).child(picture))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn browser(url: &str, title: &str) -> PageBrowser {
        PageBrowser {
            pane: 1,
            owner: 1,
            agent: "claude".into(),
            url: url.into(),
            title: title.into(),
            tabs: Vec::new(),
            shown: None,
            following: true,
            held: false,
            driver: None,
            cursor: argus_protocol::BrowserCursor::Default,
            dialog: None,
            select: None,
        }
    }

    #[test]
    fn a_page_is_said_by_its_title_and_address_or_the_address_alone() {
        assert_eq!(where_of(&browser("https://a.test/x", "A page")), (Some("A page"), "https://a.test/x"));
        assert_eq!(where_of(&browser("https://a.test/x", "")), (None, "https://a.test/x"));
        assert_eq!(where_of(&browser("https://a.test/x", "https://a.test/x")), (None, "https://a.test/x"), "a title that is the address is said once");
        assert_eq!(where_of(&browser("", "")), (None, "about:blank"));
    }
}
