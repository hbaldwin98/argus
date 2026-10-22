//! The full-transcript reader: one session read whole, in the sessions dialog in place of its list.
//! The person's turns are a jump list beside what was said; a find field marks the entries it
//! matches (TRANSCRIPT-1..16).
//!
//! Where the reader is, what it found and which entry it last went to are core's (`ReaderPage`); this
//! draws them and keeps what only a window has: the two scroll handles, the find field's text while
//! it is typed in, and what the scroll last followed. The body opens at its end when a session is
//! newly read, scrolls the entry a move went to to its top whenever the move counter changes (so a
//! move to where the page already is still scrolls back), and otherwise stays where it was scrolled.
//! The turn lit in the jump list is brought into view nearest, when it changes.

pub mod entry;
mod head;
mod turns;

use argus_desktop_core::agents::said::earlier;
use argus_desktop_core::connection::page::{Call, ReaderPage};
use argus_desktop_core::keys::keymap::{Action, Place};
use argus_desktop_core::system::clock::Clock;
use gpui_kit::component::input::InputEvent;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, Context, Entity, Focusable as _, FocusHandle, ScrollHandle, SharedString, Subscription, TestSupportExt as _, Window};

use super::ask::ask;
use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::keys::field::{Fields, Registered};
use crate::kit::field::Field;
use crate::kit::ghosts::ghosts;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;
use entry::{entry as line, Mark};

/// The widths of the loading bars of a transcript's body, as the page draws them.
const BARS: [f32; 6] = [0.5, 0.9, 0.8, 0.4, 0.85, 0.6];

/// What the scroll last followed: the session read and the move counter.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct Placed {
    session: Option<String>,
    moves: u32,
    turn: Option<usize>,
}

/// What the scroll does for a page, given what it last followed (TRANSCRIPT-10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scroll {
    Stay,
    /// A transcript newly read opens on how it ended.
    End,
    /// A move went to this entry: its top is the viewport's.
    Entry(usize),
}

/// Which scroll `page` asks for after `placed`.
fn scroll_for(page: &ReaderPage, placed: &Placed) -> Scroll {
    let read = (!page.session.loading).then(|| page.session.session_id.clone());
    match page.target {
        Some(target) if page.moves != placed.moves => Scroll::Entry(target),
        _ if read != placed.session => Scroll::End,
        _ => Scroll::Stay,
    }
}

/// Where entry `target` is among the body's children: the note of the earlier entries, if drawn, is the first, and
/// an entry of a kind that is not drawn has no child.
fn child_index(page: &ReaderPage, target: usize) -> usize {
    let note = usize::from(page.session.earlier > 0);
    note + page.session.entries.iter().take(target).filter(|line| entry::draws(line)).count()
}

pub struct ReaderView {
    pub(super) heard: Entity<Heard>,
    pub(super) caller: Caller,
    pub(super) find: Field,
    body: ScrollHandle,
    jumps: ScrollHandle,
    placed: Placed,
    /// What the field held when the session was last told, so a reader that opens with its own find
    /// in the field says nothing.
    known: SharedString,
    _typing: Subscription,
    _field: Registered,
}

impl ReaderView {
    pub fn new(heard: Entity<Heard>, caller: Caller, rest: FocusHandle, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let find = Field::new("Find in this transcript", window, cx);
        let typing = cx.subscribe(find.input(), |this, input, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                let text = input.read(cx).value();
                if text != this.known {
                    this.known = text.clone();
                    ask(&this.heard, &this.caller, Call::FindInTranscript { find: text.to_string() }, cx);
                }
            }
        });
        // Enter and Escape give the reader the keys back, on the match found.
        let back = rest.clone();
        let field = Fields::of(cx).register_place(window, find.input().read(cx).focus_handle(cx), Place::Filter, move |action, window, cx| {
            if action != Action::Leave {
                return false;
            }
            window.focus(&back, cx);
            true
        });
        Self { heard, caller, find, body: ScrollHandle::new(), jumps: ScrollHandle::new(), placed: Placed::default(), known: SharedString::default(), _typing: typing, _field: field }
    }

    /// `/` or Ctrl F: the find field takes the keys.
    pub fn focus_find(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.find.focus(window, cx);
    }

    /// Follows the session: the field says what the reader's find does while nobody is typing in
    /// it, and the scroll follows the page. A reader that goes lets its find go.
    pub fn follow(&mut self, page: Option<&ReaderPage>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(page) = page else {
            self.placed = Placed::default();
            self.known = SharedString::default();
            self.find.put("", window, cx);
            return;
        };
        if self.find.show(&page.find, window, cx) {
            self.known = SharedString::from(page.find.clone());
        }
        match scroll_for(page, &self.placed) {
            Scroll::Stay => {}
            Scroll::End => self.body.scroll_to_bottom(),
            Scroll::Entry(target) => self.body.scroll_to_top_of_item(child_index(page, target)),
        }
        let turn = page.jumps.iter().position(|jump| jump.on);
        if turn != self.placed.turn {
            if let Some(turn) = turn {
                self.jumps.scroll_to_item(turn + 1);
            }
        }
        self.placed = Placed { session: (!page.session.loading).then(|| page.session.session_id.clone()), moves: page.moves, turn };
    }

    fn body(&self, page: &ReaderPage, clock: &Clock, tokens: &Tokens) -> gpui_kit::AnyElement {
        let session = &page.session;
        if session.loading {
            return ghosts("reader-loading", &BARS, tokens).into_any_element();
        }
        if let Some(notice) = &session.notice {
            return super::sessions::said("reader-notice", notice, tokens).into_any_element();
        }
        let matched: std::collections::HashSet<usize> = page.matches.iter().copied().collect();
        v_flex()
            .id("reader-body")
            .test_support()
            .aria_label("Transcript")
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.body)
            .px(rem(tokens.sp_16))
            .py(rem(tokens.sp_12))
            .gap(rem(tokens.sp_8))
            .when(session.earlier > 0, |body| {
                let note = SharedString::from(earlier(session.earlier, true));
                body.child(div().id("reader-earlier").test_support().aria_label(note.clone()).flex_shrink_0().text_center().typed(tokens.type_small).text_color(tokens.fg_muted).child(note))
            })
            .children(session.entries.iter().enumerate().filter_map(|(at, said)| {
                let mark = if page.found == Some(at) {
                    Mark::Found
                } else if matched.contains(&at) {
                    Mark::Matched
                } else {
                    Mark::None
                };
                line(("reader-entry", at), said, mark, clock, tokens)
            }))
            .into_any_element()
    }
}

impl Render for ReaderView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        let Some(page) = self.heard.read(cx).sessions.as_ref().and_then(|sessions| sessions.reader.clone()) else {
            return div().into_any_element();
        };
        let clock = Clock::local();
        v_flex()
            .id("reader")
            .test_support()
            .size_full()
            .child(self.head(&page, &clock, &tokens, cx))
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .child(self.turns(&page, &clock, &tokens, cx))
                    .child(div().flex_1().min_w_0().h_full().bg(tokens.bg_sunken).child(self.body(&page, &clock, &tokens))),
            )
            .child(head::keys(&page, &tokens))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests;
