//! The recent-sessions dialog: a repository's agent sessions, live ones first, beside what the one
//! under the cursor said, with the way to go to it, resume it or read it whole in the reader that
//! takes the list's place (SESSIONS-1..25).
//!
//! Which sessions are listed, the search and filters over them, the cursor and what each session
//! offers are core's (`SessionsPage`); the dialog is up while the session holds it
//! (`Heard::sessions`) and every press sends the call the page sends and changes nothing on screen:
//! a refused action leaves the dialog showing what is true. What only a window keeps is here: the
//! search field's text while it is typed in, the list's and the preview's scroll, and where the keys
//! rest. The list keeps its offset while the reader is up, so closing the reader puts the person
//! back where they were; the preview opens on how the session ended, as the page's does.
//!
//! The dialog is not dismissed by the scrim or by Escape of the host: Escape is the dialog's layer
//! (`keys`), which asks the session to close, and the session's word that it is shut takes it away.

mod filter;
mod head;
pub mod keys;
mod list;
mod preview;

use argus_desktop_core::agents::sessions_dialog::SessionsPage;
use argus_desktop_core::connection::page::Call;
use argus_desktop_core::keys::keymap::{Action, Place};
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::InputEvent;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, App, Context, Entity, FocusHandle, Focusable as _, Global, ScrollHandle, SharedString, Subscription, TestSupportExt as _, Window};

use super::ask::ask;
use super::transcript::ReaderView;
use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::keys::field::{Fields, Registered};
use crate::kit::field::Field;
use crate::kit::overlay::host::{Dismiss, Overlays, Spec};
use crate::kit::overlay::kind::Overlay;
use crate::style::Typed as _;
use crate::theme::scale::{pixels, rem};
use crate::theme::Tokens;

/// The dialog's view, kept by the window for as long as it runs.
#[derive(Clone)]
pub struct Held(pub Entity<SessionsView>);

impl Global for Held {}

/// `.review-empty.fg-muted`: a quiet line standing where a pane is not drawn.
pub fn said(id: &'static str, words: &str, tokens: &Tokens) -> impl IntoElement {
    div()
        .id(id)
        .test_support()
        .aria_label(SharedString::from(words.to_owned()))
        .size_full()
        .p(rem(tokens.sp_24))
        .typed(tokens.type_body)
        .text_color(tokens.fg_muted)
        .child(SharedString::from(words.to_owned()))
}

pub struct SessionsView {
    pub(super) heard: Entity<Heard>,
    pub(super) caller: Caller,
    pub(super) search: Field,
    /// What the field held when the session was last told.
    known: SharedString,
    /// Where the dialog's keys rest while neither field has them.
    pub(super) rest: FocusHandle,
    pub(super) reader: Entity<ReaderView>,
    pub(super) list: ScrollHandle,
    pub(super) body: ScrollHandle,
    /// The session whose transcript the preview last drew read, and the row the list last scrolled to.
    read: Option<String>,
    lit: Option<(usize, usize)>,
    /// Whether this view's dialog is up; the host is an app global, so another window's open
    /// Sessions overlay says nothing about this one.
    up: bool,
    _typing: Subscription,
    _field: Registered,
}

impl SessionsView {
    pub fn new(heard: Entity<Heard>, caller: Caller, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let rest = cx.focus_handle();
        let search = Field::new("Search sessions", window, cx);
        let typing = cx.subscribe(search.input(), |this, input, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                let text = input.read(cx).value();
                if text != this.known {
                    this.known = text.clone();
                    ask(&this.heard, &this.caller, Call::SearchSessions { search: text.to_string() }, cx);
                }
            }
        });
        // Enter and Escape give the list the keys back, filtered; Escape closes nothing from here.
        let back = rest.clone();
        let field = Fields::of(cx).register_place(window, search.input().read(cx).focus_handle(cx), Place::Filter, move |action, window, cx| {
            if action != Action::Leave {
                return false;
            }
            window.focus(&back, cx);
            true
        });
        let reader = cx.new(|cx| ReaderView::new(heard.clone(), caller.clone(), rest.clone(), window, cx));
        Self {
            heard,
            caller,
            search,
            known: SharedString::default(),
            rest,
            reader,
            list: ScrollHandle::new(),
            body: ScrollHandle::new(),
            read: None,
            lit: None,
            up: false,
            _typing: typing,
            _field: field,
        }
    }

    /// `/`: the search field takes the keys.
    pub fn focus_search(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.search.focus(window, cx);
    }

    /// `/` or Ctrl F in the reader: the find field takes the keys.
    pub fn focus_find(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.reader.update(cx, |reader, cx| reader.focus_find(window, cx));
    }

    /// Whether the full transcript is open in place of the list.
    pub fn reading(&self, cx: &App) -> bool {
        self.heard.read(cx).sessions.as_ref().is_some_and(|page| page.reader.is_some())
    }

    /// Follows the session: the dialog goes up while it holds one, and its parts follow the page.
    pub fn follow(view: &Entity<Self>, window: &mut Window, cx: &mut App) {
        let page = view.read(cx).heard.read(cx).sessions.clone();
        let host = Overlays::get(cx);
        let Some(page) = page else {
            if view.read(cx).up {
                host.close(Overlay::Sessions, window, cx);
            }
            view.update(cx, |this, cx| {
                this.up = false;
                this.reader.update(cx, |reader, cx| reader.follow(None, window, cx));
                this.search.put("", window, cx);
                this.known = SharedString::default();
                this.read = None;
                this.lit = None;
            });
            return;
        };
        view.update(cx, |this, cx| this.page(&page, window, cx));
        if !view.read(cx).up {
            view.update(cx, |this, _| this.up = true);
            let (shown, rest) = (view.clone(), view.read(cx).rest.clone());
            host.open(
                Spec::new(Overlay::Sessions).dismissed(Dismiss::Never),
                move |dialog: Dialog, window, cx, first| {
                    if first {
                        window.focus(&rest, cx);
                    }
                    let gutter = pixels(Tokens::global(cx).sp_24, window.rem_size());
                    dialog.w(window.viewport_size().width - gutter * 2.0).margin_top(gutter).p_0().on_ok(|_, _, _| false).child(shown.clone())
                },
                window,
                cx,
            );
        }
    }

    /// What the page says to the parts: the search field while nobody is typing in it, the reader,
    /// and the scrolls.
    fn page(&mut self, page: &SessionsPage, window: &mut Window, cx: &mut Context<Self>) {
        if self.search.show(&page.search, window, cx) {
            self.known = SharedString::from(page.search.clone());
        }
        self.reader.update(cx, |reader, cx| reader.follow(page.reader.as_ref(), window, cx));
        // A transcript newly read in the preview opens on how it ended; one that is redrawn stays
        // where it was scrolled. The reader hides the preview, so the next draw is newly read again
        // (SESSIONS-17, -24).
        let read = match (&page.preview, &page.reader) {
            (Some(preview), None) if !preview.loading => Some(preview.session_id.clone()),
            _ => None,
        };
        if read.is_some() && read != self.read {
            self.body.scroll_to_bottom();
        }
        self.read = read;
        cx.notify();
    }
}

impl Render for SessionsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        let Some(page) = self.heard.read(cx).sessions.clone() else { return div().into_any_element() };
        let height = window.viewport_size().height - pixels(2.0 * tokens.sp_24, window.rem_size());
        let body = if page.reader.is_some() {
            self.reader.clone().into_any_element()
        } else {
            v_flex()
                .size_full()
                .child(self.head(&page, &tokens, cx))
                .child(h_flex().flex_1().min_h_0().child(self.list_column(&page, window, &tokens, cx)).child(self.preview(&page, &tokens, cx)))
                .child(self.footer(&page, &tokens, cx))
                .into_any_element()
        };
        div()
            .id("sessions")
            .role(gpui_kit::Role::Dialog)
            .test_support()
            .aria_label(if page.reader.is_some() { "Transcript" } else { "Recent sessions" })
            .track_focus(&self.rest)
            .w_full()
            .h(height)
            .child(body)
            .into_any_element()
    }
}

#[cfg(test)]
pub(crate) mod tests;
