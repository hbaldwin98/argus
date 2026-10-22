//! What the Checkouts stage puts in the strip: the counts beside the title, and at its end the
//! filter field, `New worktree` and the `⋯` with the branches and a fetch of all (`ck/stage.ts`,
//! `checkoutsStrip`). With no repository chosen the strip carries neither.
//!
//! The filter sends `FilterCheckouts` with the text as it stands on every edit, and the session
//! matches (the rows change when its next table says so). The field's text is replaced from the
//! table only while the field has not the keys, so a word half typed is never overwritten and its
//! caret holds across the tables the typing brings back. Enter and Escape in the field (`Leave` of
//! the `Filter` place) give the keys back to the stage, the filter kept (CHECKOUTS-12).

use argus_desktop_core::checkouts::table::Checkouts;
use argus_desktop_core::checkouts::words::{counts, COUNTS_TIP};
use argus_desktop_core::checkouts::ways;
use argus_desktop_core::chrome::strip::{Part, Says};
use argus_desktop_core::connection::page::Call;
use argus_desktop_core::keys::keymap::{Action, Place};
use gpui_kit::component::input::{Input, InputEvent};
use gpui_kit::component::menu::DropdownMenu as _;
use gpui_kit::component::h_flex;
use gpui_kit::{div, prelude::*, Anchor, Context, Entity, FocusHandle, Focusable as _, MouseButton, Role, SharedString, Subscription, TestSupportExt as _, Window};

use super::{act, menus};
use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::keys::field::{Fields, Registered};
use crate::kit::button::{button, Look};
use crate::kit::field::Field;
use crate::kit::icons::Glyph;
use crate::kit::strip::menu_button;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// What the strip says of the stage: the counts, unfiltered, and ` · primary <main>` while the
/// repository has a main branch.
pub fn says(table: Option<&Checkouts>) -> Says {
    let Some(table) = table.filter(|table| table.repository.is_some()) else { return Says::default() };
    let counts = counts(table);
    let mut meta = vec![Part::Text(counts.totals)];
    if let Some(main) = counts.main {
        meta.push(Part::Text(" \u{b7} primary ".into()));
        meta.push(Part::Mono(main));
    }
    Says { meta, meta_tip: Some(COUNTS_TIP), ..Says::default() }
}

/// The strip's tools.
pub struct StripTools {
    heard: Entity<Heard>,
    caller: Caller,
    field: Field,
    /// What the session was last told the filter is, so a field put back to what the table says
    /// sends nothing.
    known: SharedString,
    _subscriptions: [Subscription; 2],
    _field: Registered,
}

impl StripTools {
    /// `stage` is where the keys go when the field lets go of them.
    pub fn new(heard: Entity<Heard>, caller: Caller, stage: FocusHandle, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let field = Field::new("Filter checkouts and branches", window, cx);
        let typing = cx.subscribe(field.input(), |this, input, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                let text = input.read(cx).value();
                if text != this.known && !this.heard.read(cx).held() {
                    this.known = text.clone();
                    this.caller.call(Call::FilterCheckouts { filter: text.to_string() });
                }
            }
        });
        let following = cx.observe_in(&heard, window, |this, heard, window, cx| {
            let said = heard.read(cx).checkouts.as_ref().map(|table| table.filter.clone());
            if let Some(said) = said {
                // Known first, so putting what the table says into the field is not an edit to send;
                // a field that is being typed in is left alone, and what was known stays.
                let before = std::mem::replace(&mut this.known, SharedString::from(said.clone()));
                if !this.field.show(&said, window, cx) && this.field.text(cx) != said {
                    this.known = before;
                }
            }
        });
        let keys = Fields::of(cx).register_place(window, field.input().focus_handle(cx), Place::Filter, move |action, window, cx| {
            if action != Action::Leave {
                return false;
            }
            window.focus(&stage, cx);
            true
        });
        Self { heard, caller, field, known: SharedString::default(), _subscriptions: [typing, following], _field: keys }
    }

    /// `/`: the field takes the keys. Nothing is sent: the page handles it itself.
    pub fn focus(&self, window: &mut Window, cx: &mut gpui_kit::App) {
        self.field.focus(window, cx);
    }
}

impl Render for StripTools {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        let table = self.heard.read(cx).checkouts.clone();
        if table.as_ref().is_none_or(|table| table.repository.is_none()) {
            return div().into_any_element();
        }
        let (caller, heard) = (self.caller.clone(), self.heard.clone());
        let (more_caller, more_heard) = (self.caller.clone(), self.heard.clone());
        h_flex()
            .id("checkouts-tools")
            .gap(rem(tokens.sp_8))
            .items_center()
            // `.head-filter`: a plain field with no mark in it, the `/` that takes it being in the key bar.
            .child(
                div().id("checkouts-filter").test_support().w(rem(tokens.ck_filter_w)).flex_shrink_0().child(
                    Input::new(self.field.input()).id("checkouts-filter-field").h(rem(tokens.control_h)).aria_label("Filter checkouts and branches"),
                ),
            )
            .child(button("new-worktree", "New worktree", Look::Primary, false, &tokens).child("New worktree").on_click(move |_, window, cx| {
                let table = heard.read(cx).checkouts.clone();
                let then = ways::new_worktree(table.as_ref().and_then(|table| table.selected_row()));
                act::run(then, &caller, &heard, window, cx);
            }))
            .child(
                div()
                    .id("checkouts-more")
                    .role(Role::Button)
                    .test_support()
                    .aria_label("More about checkouts")
                    .flex_shrink_0()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        menu_button("checkouts-more-button", Glyph::More, tokens.icon_sm, &tokens, cx)
                            .tooltip("More: branches, fetch all")
                            .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, window, cx| menus::strip(menu, &more_caller, &more_heard, tokens, window, cx)),
                    ),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_counts_name_the_main_branch_in_mono_only_while_there_is_one() {
        let table = Checkouts { repository: Some("argus".into()), checkouts: 4, branches: 9, main: Some("main".into()), ..Checkouts::default() };
        let said = says(Some(&table));
        assert_eq!(said.meta, [Part::Text("4 checkouts \u{b7} 9 other branches".into()), Part::Text(" \u{b7} primary ".into()), Part::Mono("main".into())]);
        assert_eq!(said.meta_tip, Some("Enter open \u{b7} D delete"));
        assert_eq!(says(Some(&Checkouts { main: None, ..table })).meta.len(), 1);
    }

    #[test]
    fn with_no_table_or_no_repository_the_strip_says_nothing() {
        assert!(says(None).meta.is_empty());
        assert!(says(Some(&Checkouts::default())).meta.is_empty());
    }
}
