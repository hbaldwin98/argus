//! The filter field of the panel: what is typed is sent as the filter, except a `#`, which turns
//! the text to a search of the checkout and filters no names; Enter on such a text sends the
//! search, and Enter or Escape on any other gives the keys back to the tree (`tools`,
//! `ck/files.ts`).
//!
//! The rule for the `#` is core's one `Query`, the same the Code stage's field reads.

use argus_desktop_core::checkouts::files::FilesGesture;
use argus_desktop_core::checkouts::query::Query;
use argus_desktop_core::keys::keymap::{Action, Place};
use gpui_kit::component::h_flex;
use gpui_kit::component::input::InputEvent;
use gpui_kit::{prelude::*, Context, FocusHandle, Focusable as _, SharedString, Subscription, TestSupportExt as _, Window};

use super::{FilesPanel, FILTER};
use crate::keys::field::{Fields, Registered};
use crate::keys::layer::{action_for, Verdict};
use crate::kit::field::Field;
use crate::kit::finder::{cap, finder};
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// The filter's subscription to its edits and its place in the key walk. `tree` is where the keys
/// go when the field lets go of them.
pub(super) fn wire(filter: &Field, tree: &FocusHandle, window: &mut Window, cx: &mut Context<FilesPanel>) -> (Subscription, Registered) {
    let typing = cx.subscribe(filter.input(), |this, input, event: &InputEvent, cx| {
        if !matches!(event, InputEvent::Change) {
            return;
        }
        let text = input.read(cx).value();
        let filter = Query::of(&text).filter().to_owned();
        if filter != this.known.as_ref() {
            this.known = SharedString::from(filter.clone());
            this.ask(FilesGesture::Filter { filter }, cx);
        }
    });
    let (panel, tree) = (cx.weak_entity(), tree.clone());
    let field = filter.clone();
    let keys = Fields::of(cx).register(window, filter.input().focus_handle(cx), move |press, window, cx| {
        // Enter on a `#` with something after it searches; anything else of Enter or Escape leaves.
        if action_for(&[Place::Jump], press.page) == Some(Action::Choose) {
            if let Some(query) = Query::of(&field.text(cx)).to_search().map(str::to_owned) {
                panel.update(cx, |panel, cx| panel.ask(FilesGesture::Search { query }, cx)).ok();
                return Verdict::Taken;
            }
        }
        if action_for(&[Place::Filter], press.page) == Some(Action::Leave) {
            window.focus(&tree, cx);
            return Verdict::Taken;
        }
        Verdict::Held
    });
    (typing, keys)
}

impl FilesPanel {
    pub(super) fn tools(&self, tokens: &Tokens) -> impl IntoElement {
        // `.cf-tools`: 12px each side, 8 under.
        h_flex()
            .id("files-tools")
            .test_support()
            .flex_shrink_0()
            .px(rem(tokens.sp_12))
            .pb(rem(tokens.sp_8))
            .child(finder("files-filter", self.filter.input(), FILTER, Some(cap("/", tokens)), tokens))
    }
}
