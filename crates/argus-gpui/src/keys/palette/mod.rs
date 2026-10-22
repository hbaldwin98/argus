//! The command palette (SHELL-PALETTE-1..13): one search over the commands and what the session says
//! it can go to, grouped, each result with its key or its status beside it. `>` narrows the search to
//! commands and `#` to features.
//!
//! What is listed and in what order is core's (`keys/results` makes the rows, `keys/rank` keeps and
//! orders them); this draws them and runs the keys of the `palette` place in the search field:
//! Down and Up move the highlight and wrap, Enter chooses, Escape closes. The palette runs nothing
//! itself: choosing is a call. It tells the session when it opens and closes, so the session sends
//! what it can go to only while it is open. The highlight is tracked by the row's stable key, so
//! a list the session sends again while the palette is open does not move it from under Enter.
//!
//! "Tab next group" is in the footer and does nothing, as in the page (audit §5, question 8).

mod rows;

use argus_desktop_core::connection::page::Call;
use argus_desktop_core::keys::chords::leader;
use argus_desktop_core::keys::keymap::{Action, Place};
use argus_desktop_core::keys::rank::{rank, Group, Ranked};
use argus_desktop_core::keys::results::{highlight, nothing, results, Result};
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, App, Context, Entity, Focusable as _, Role, ScrollHandle, SharedString, Subscription, TestSupportExt as _, Window};
use std::rc::Rc;

use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::keys::field::{Fields, Registered};
use crate::kit::icons::Glyph;
use crate::kit::keycap::keycap;
use crate::kit::overlay::host::{Dismiss, Overlays, Spec};
use crate::kit::overlay::kind::Overlay;
use crate::style::Typed as _;
use crate::theme::scale::{pixels, rem};
use crate::theme::Tokens;

pub struct PaletteView {
    heard: Entity<Heard>,
    caller: Caller,
    input: Entity<InputState>,
    /// The highlighted row's place among those shown, and its key, which keeps it where it is while
    /// the lists change.
    at: usize,
    held: Option<String>,
    /// The highlight moved by a key, so its row is scrolled into view once; a pointer moving it is on
    /// a row already in view.
    reveal: bool,
    scroll: ScrollHandle,
    _subscriptions: Vec<Subscription>,
    _keys: Registered,
}

impl PaletteView {
    pub fn new(heard: Entity<Heard>, caller: Caller, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Search, or > for commands, # for features"));
        let typing = cx.subscribe(&input, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                // Typing puts the highlight on the first result.
                this.at = 0;
                this.held = None;
                cx.notify();
            }
        });
        let observing = cx.observe(&heard, |_, _, cx| cx.notify());
        let me = cx.weak_entity();
        let keys = Fields::of(cx).register_place(window, input.focus_handle(cx), Place::Palette, move |action, window, cx| {
            me.update(cx, |this, cx| {
                match action {
                    Action::Next => this.step(true, cx),
                    Action::Previous => this.step(false, cx),
                    Action::Choose => this.choose_highlighted(window, cx),
                    Action::CloseLayer => {
                        this.close(window, cx);
                    }
                    _ => return false,
                }
                true
            })
            .unwrap_or(false)
        });
        Self { heard, caller, input, at: 0, held: None, reveal: false, scroll: ScrollHandle::new(), _subscriptions: vec![typing, observing], _keys: keys }
    }

    /// Opens it, empty, on the first result, and says so to the session; opening one that is open
    /// does nothing (SHELL-PALETTE-1).
    pub fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (view, input, closing) = (cx.entity(), self.input.clone(), self.caller.clone());
        let opened = Overlays::get(cx).open(
            Spec::new(Overlay::Palette).dismissed(Dismiss::Answers(Rc::new(move |_, _| closing.call(Call::Palette { open: false })))),
            move |dialog: Dialog, window, cx, first| {
                if first {
                    input.update(cx, |input, cx| input.focus(window, cx));
                }
                let (tokens, height) = (*Tokens::global(cx), window.viewport_size().height);
                dialog.w(pixels(tokens.palette_w, window.rem_size())).p_0().margin_top(height * tokens.palette_top_share).on_ok(|_, _, _| false).child(view.clone())
            },
            window,
            cx,
        );
        if opened {
            self.at = 0;
            self.held = None;
            self.input.update(cx, |input, cx| input.set_value("", window, cx));
            self.caller.call(Call::Palette { open: true });
            cx.notify();
        }
    }

    /// Puts it away and tells the session, so it stops sending (SHELL-PALETTE-2).
    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let closed = Overlays::get(cx).close(Overlay::Palette, window, cx);
        if closed {
            self.caller.call(Call::Palette { open: false });
        }
        closed
    }

    /// Every row, and which of them the query keeps.
    fn shown(&self, cx: &App) -> (Vec<Result>, Ranked) {
        let heard = self.heard.read(cx);
        let rows = results(&heard.commands, &heard.extension_views, &heard.extension_commands, heard.palette.as_ref());
        let ranked = rank(&self.input.read(cx).value(), &rows.iter().map(Result::candidate).collect::<Vec<_>>());
        (rows, ranked)
    }

    /// Down and Up move the highlight and wrap around (SHELL-PALETTE-9).
    fn step(&mut self, down: bool, cx: &mut Context<Self>) {
        let (rows, ranked) = self.shown(cx);
        let flat = ranked.flat();
        let count = flat.len();
        if count == 0 {
            return;
        }
        self.at = (self.at + if down { 1 } else { count - 1 }) % count;
        self.held = Some(rows[flat[self.at].at].key.clone());
        self.reveal = true;
        cx.notify();
    }

    fn choose_highlighted(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (rows, ranked) = self.shown(cx);
        if let Some(hit) = ranked.flat().get(self.at) {
            self.choose(&rows[hit.at], window, cx);
        }
    }

    /// Closes, then asks for what the row is: a pane gone to is also typed in (SHELL-PALETTE-9).
    fn choose(&mut self, row: &Result, window: &mut Window, cx: &mut Context<Self>) {
        self.close(window, cx);
        let picked = self.heard.read(cx).rail.cursor.pick.map(|pick| (pick.checkout, pick.pane));
        self.caller.call(row.does.call(picked));
        if row.does.gives_the_pane_the_keys() {
            // After the dialog has gone and the kit has handed the keys back to what had them.
            window.defer(cx, |window, cx| Overlays::get(cx).focus_pane(window, cx));
        }
    }

    fn highlight_at(&mut self, at: usize, key: String, cx: &mut Context<Self>) {
        if self.at != at {
            self.at = at;
            self.held = Some(key);
            cx.notify();
        }
    }

    /// The footer's hints: the caps, then what they do.
    fn hint(keys: &[&'static str], words: &'static str, tokens: &Tokens) -> impl IntoElement {
        let mono = SharedString::from(tokens.font_mono);
        h_flex().gap(rem(tokens.sp_6)).items_center().children(keys.iter().map(|key| keycap(*key, mono.clone(), tokens))).child(words)
    }
}

impl Render for PaletteView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        let (results, ranked) = self.shown(cx);
        let flat_keys: Vec<&str> = ranked.flat().iter().map(|hit| results[hit.at].key.as_str()).collect();
        self.at = highlight(&flat_keys, self.held.as_deref(), self.at);
        self.held = flat_keys.get(self.at).map(|key| (*key).to_owned());
        let leader = leader(&self.heard.read(cx).keymap);
        let features_only = ranked.scope == Some(Group::Features);
        let says = nothing(features_only, self.heard.read(cx).palette.as_ref());

        let mut children = Vec::new();
        let (mut at, mut highlighted_child) = (0, 0);
        for (group, hits) in &ranked.groups {
            children.push(rows::heading(*group, &tokens).into_any_element());
            for hit in hits {
                let row = &results[hit.at];
                let on = at == self.at;
                if on {
                    highlighted_child = children.len();
                }
                let (key, here) = (row.key.clone(), at);
                children.push(
                    rows::row(at, row, &hit.marks, on, &leader, &tokens)
                        .on_mouse_move(cx.listener(move |this, _, _, cx| this.highlight_at(here, key.clone(), cx)))
                        .on_click(cx.listener({
                            let row = row.clone();
                            move |this, _, window, cx| this.choose(&row, window, cx)
                        }))
                        .into_any_element(),
                );
                at += 1;
            }
        }
        if children.is_empty() {
            children.push(div().id("palette-empty").test_support().aria_label(says).px(rem(tokens.sp_8)).py(rem(tokens.sp_12)).typed(tokens.type_small).text_color(tokens.fg_muted).child(says).into_any_element());
        }
        // The highlighted row is scrolled into view.
        if std::mem::take(&mut self.reveal) {
            self.scroll.scroll_to_item(highlighted_child);
        }
        let (head_h, foot_h) = (tokens.sp_44, tokens.control_h + 2.0 * tokens.sp_12);
        let room = window.viewport_size().height * tokens.palette_max_share - pixels(head_h + foot_h, window.rem_size());
        v_flex()
            .id("palette")
            .role(Role::Dialog)
            .test_support()
            .aria_label("Palette")
            .child(
                h_flex()
                    .flex_shrink_0()
                    .h(rem(tokens.sp_44))
                    .px(rem(tokens.sp_16))
                    .gap(rem(tokens.sp_12))
                    .items_center()
                    .border_b_1()
                    .border_color(tokens.line)
                    .text_color(tokens.fg_muted)
                    .child(Glyph::Search.at(tokens.icon_sm))
                    .child(
                        div().flex_1().min_w_0().text_color(tokens.fg).child(
                            Input::new(&self.input)
                                .id("palette-field")
                                .appearance(false)
                                .bordered(false)
                                .focus_bordered(false)
                                .text_size(rem(tokens.fs_lg))
                                .aria_label("Search actions, panes, features and checkouts"),
                        ),
                    )
                    .child(keycap("Esc", SharedString::from(tokens.font_mono), &tokens)),
            )
            .child(
                v_flex()
                    .id("palette-results")
                    .role(Role::ListBox)
                    .max_h(room)
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .p(rem(tokens.sp_4))
                    .children(children),
            )
            .child(
                h_flex()
                    .flex_shrink_0()
                    .px(rem(tokens.sp_16))
                    .py(rem(tokens.sp_12))
                    .gap(rem(tokens.sp_16))
                    .items_center()
                    .border_t_1()
                    .border_color(tokens.line)
                    .typed(tokens.type_small)
                    .text_color(tokens.fg_muted)
                    .child(Self::hint(&["\u{2191}", "\u{2193}"], "move", &tokens))
                    .child(Self::hint(&["\u{21b5}"], "open", &tokens))
                    .child(Self::hint(&["Tab"], "next group", &tokens))
                    .child(div().flex_1())
                    .child(Self::hint(&[">"], "commands", &tokens))
                    .child(Self::hint(&["#"], "features", &tokens)),
            )
    }
}

#[cfg(test)]
mod tests;
