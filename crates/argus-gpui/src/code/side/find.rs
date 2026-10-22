//! The one field over the tree: a name goes to a file, a leading `#` turns it to a search of the
//! text, and while it is a search its three options sit at its right (`find`, `option`,
//! `code/stage.ts`). Rust holds what it shows (`code::side::field_text`) and decides which of the
//! two lists the text is, so every edit is sent as it stands to whichever list is open.
//!
//! The field's keys are the `Jump` place's and, while it is a search, the `Filter` place's Alt
//! chords; core says what each comes to (`field_key`). Reaching the field with no list open opens
//! the go-to list.

use argus_desktop_core::code::side::{field_key, option_click, options, body, field_text, keeps_typed, Body, SearchOption, FIND_HINT, FIND_KEY};
use argus_desktop_core::code::stage::{CodeGesture, CodePage};
use argus_desktop_core::connection::page::ReviewJumpGesture;
use argus_desktop_core::keys::keymap::{Action, Place};
use gpui_kit::component::h_flex;
use gpui_kit::component::input::InputEvent;
use gpui_kit::{div, prelude::*, transparent_black, AnyElement, Context, Entity, Focusable as _, Role, SharedString, Subscription, TestSupportExt as _, Window};

use crate::code::stage::CodeStage;
use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::keys::field::{Fields, Registered};
use crate::kit::field::Field;
use crate::keys::layer::{action_for, Verdict};
use crate::kit::finder::{cap, finder};
use crate::kit::tip::tip;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// The find field's subscription to its edits and focus, and its place in the key walk.
pub(in crate::code) fn wire(find: &Field, heard: &Entity<Heard>, caller: &Caller, window: &mut Window, cx: &mut Context<CodeStage>) -> (Subscription, Registered) {
    let input = find.input().clone();
    let typing = cx.subscribe(&input, |this, input, event: &InputEvent, cx| match event {
        InputEvent::Change => {
            let text = input.read(cx).value();
            if text != this.known_find {
                this.known_find = text.clone();
                let Some(page) = this.heard.read(cx).code.clone() else { return };
                let gesture = ReviewJumpGesture::Query { text: text.to_string() };
                this.send(if page.search.is_some() { CodeGesture::Search { gesture } } else { CodeGesture::Jump { gesture } }, cx);
            }
        }
        // The field is always there; reaching it opens the list it filters.
        InputEvent::Focus => {
            let Some(page) = this.heard.read(cx).code.clone() else { return };
            if body(&page) == Body::Tree {
                this.send(CodeGesture::Action { action: Action::Filter }, cx);
            }
        }
        _ => {}
    });
    let (heard, caller) = (heard.clone(), caller.clone());
    let keys = Fields::of(cx).register(window, input.focus_handle(cx), move |press, _, cx| {
        let Some(action) = action_for(&[Place::Jump, Place::Filter], press.page) else { return Verdict::Held };
        let Some(page) = heard.read(cx).code.clone() else { return Verdict::Held };
        match field_key(page.search.is_some(), action) {
            Some(gesture) if !heard.read(cx).held() => {
                caller.call(argus_desktop_core::connection::page::Call::Code { gesture });
                Verdict::Taken
            }
            Some(_) => Verdict::Taken,
            None => Verdict::Held,
        }
    });
    (typing, keys)
}

/// `.sopt`: one of the search's toggles, `--icon-btn` square, lit with the accent edge while on.
fn option(shown: &SearchOption, tokens: &Tokens, cx: &mut Context<CodeStage>) -> AnyElement {
    let which = shown.which;
    let (fill, text) = (tokens.fill, tokens.fg);
    let base = h_flex()
        .id(SharedString::from(format!("code-opt-{which}")))
        .role(Role::Button)
        .test_support()
        .aria_label(SharedString::from(shown.title))
        .aria_selected(shown.on)
        .tooltip(tip(shown.title))
        .w(rem(tokens.icon_btn))
        .h(rem(tokens.icon_btn))
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .rounded(rem(tokens.r_xs))
        .border_1()
        .font_family(SharedString::from(tokens.font_mono))
        .text_size(rem(tokens.fs_xs))
        .cursor_pointer();
    let base = if shown.on {
        base.bg(tokens.bg_selected).border_color(tokens.accent).text_color(text)
    } else {
        base.border_color(transparent_black()).text_color(tokens.fg_muted).hover(move |style| style.bg(fill).text_color(text))
    };
    base.child(shown.label).on_click(cx.listener(move |this, _, _, cx| this.send(option_click(which), cx))).into_any_element()
}

impl CodeStage {
    pub(super) fn find(&self, page: &CodePage, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let suffix = match &page.search {
            Some(search) => {
                let shown = options(search);
                Some(h_flex().id("code-options").test_support().aria_label("How to match").gap(rem(tokens.hairline * 2.0)).children(shown.iter().map(|shown| option(shown, tokens, cx))).into_any_element())
            }
            None => Some(cap(FIND_KEY, tokens)),
        };
        div().id("code-find-box").test_support().flex_shrink_0().child(finder("code-find", self.find.input(), FIND_HINT, suffix, tokens))
    }

    /// Writes what Rust holds into the field, unless what is typed still means that: a key pressed
    /// between a gesture and its redraw is not lost, and a `#` that moved the field to the other
    /// list, or a list put away, is Rust's word.
    pub(in crate::code) fn show_find(&mut self, page: &CodePage, window: &mut Window, cx: &mut Context<Self>) {
        let typed = self.find.text(cx);
        if keeps_typed(page, &typed) {
            return;
        }
        let said = field_text(page);
        if typed == said {
            return;
        }
        self.known_find = SharedString::from(said.clone());
        self.find.put(&said, window, cx);
    }
}
