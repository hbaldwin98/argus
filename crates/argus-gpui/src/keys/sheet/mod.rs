//! The shortcuts sheet (SHELL-SHEET-1..13): every binding of the keymap the session sent, a section per
//! place with its keys beside it, opened on the section for where the person is.
//!
//! The sections, their rows, the search over them and the choice of the first section are core's
//! (`keys/sheet`); this draws them and keeps what only a window has: which section is up, what the
//! find field holds, and the keys. The sheet asks for its own keys. It takes them from the pane the
//! way the page does, by focusing its close button without a ring, so letters are held and `?` or
//! Escape put it away (`keys/layers/sheet`); the find field answers the place's Down and Up itself,
//! since a field swallows keys before the walk hears them.

mod body;

use argus_desktop_core::keys::chords::leader;
use argus_desktop_core::keys::keymap::{Action, Place};
use argus_desktop_core::keys::sheet::{all, matching, section_for, stage_id, step, Dialog as Open, Section, Where};
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::{Input, InputEvent};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, App, Context, Entity, FocusHandle, Focusable as _, Role, ScrollHandle, SharedString, Subscription, TestSupportExt as _, Window};

use crate::connection::heard::Heard;
use crate::keys::field::{Fields, Registered};
use crate::kit::dialog::{foot, head};
use crate::kit::field::Field;
use crate::kit::icons::Glyph;
use crate::kit::keycap::keycap;
use crate::kit::overlay::host::{Overlays, Spec};
use crate::kit::overlay::kind::Overlay;
use crate::kit::selected::selected;
use crate::style::Typed as _;
use crate::theme::scale::{pixels, rem};
use crate::theme::Tokens;

pub struct SheetView {
    heard: Entity<Heard>,
    find: Field,
    /// Where the keys are while the sheet is up: the close button, with no ring.
    close: FocusHandle,
    /// Every section, as the keymap was when the sheet opened.
    all: Vec<Section>,
    current: String,
    /// The section for where the person was.
    here: String,
    body: ScrollHandle,
    nav: ScrollHandle,
    _subscriptions: Vec<Subscription>,
    _keys: Registered,
}

impl SheetView {
    pub fn new(heard: Entity<Heard>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let find = Field::new("Find a key or an action", window, cx);
        let typing = cx.subscribe(find.input(), |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.body.set_offset(Default::default());
                cx.notify();
            }
        });
        let me = cx.weak_entity();
        let keys = Fields::of(cx).register_place(window, find.input().read(cx).focus_handle(cx), Place::Sheet, move |action, _, cx| {
            let down = match action {
                Action::Next => true,
                Action::Previous => false,
                _ => return false,
            };
            me.update(cx, |this, cx| this.step(down, cx)).is_ok()
        });
        Self { heard, find, close: cx.focus_handle(), all: Vec::new(), current: String::new(), here: String::new(), body: ScrollHandle::new(), nav: ScrollHandle::new(), _subscriptions: vec![typing], _keys: keys }
    }

    /// Where the person is, as the sheet reads it. The rail's picked row and the open dialogs are the
    /// session's; the keys in the pane's text area are the window's. The Feature stage's outline and
    /// diagram are not in this window yet, so neither is ever up.
    fn whereabouts(&self, window: &Window, cx: &App) -> Where {
        let heard = self.heard.read(cx);
        Where {
            pane: Overlays::get(cx).pane_has_keys(window, cx),
            dialog: if heard.review.is_some() {
                Some(Open::Review)
            } else if heard.history.is_some() {
                Some(Open::History)
            } else {
                None
            },
            rail: heard.rail.cursor.pick.is_some(),
            outline: false,
            diagram: false,
            stage: stage_id(heard.rail.stage),
        }
    }

    /// `?` opens it on the section for where the person is, and `?` again closes it (SHELL-SHEET-1).
    pub fn toggle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let host = Overlays::get(cx);
        if host.is_open(Overlay::Sheet) {
            host.close(Overlay::Sheet, window, cx);
            return;
        }
        let at = self.whereabouts(window, cx);
        let (keymap, groups) = {
            let heard = self.heard.read(cx);
            (heard.keymap.clone(), heard.extension_groups.clone())
        };
        self.all = all(&keymap, &groups);
        let ids: Vec<String> = self.all.iter().map(|section| section.id.clone()).collect();
        self.here = section_for(&at, &ids);
        self.current = self.here.clone();
        self.find.put("", window, cx);
        let (view, close) = (cx.entity(), self.close.clone());
        host.open(
            Spec::new(Overlay::Sheet),
            move |dialog: Dialog, window, cx, first| {
                // The keys are on the close button, so the pane is blurred and a letter is held.
                if first {
                    window.focus(&close, cx);
                }
                let tokens = *Tokens::global(cx);
                dialog.w(pixels(tokens.sheet_w, window.rem_size())).p_0().on_ok(|_, _, _| false).child(view.clone())
            },
            window,
            cx,
        );
        cx.notify();
    }

    /// Down and Up change section, clamped at the ends, while nothing is being searched for.
    fn step(&mut self, down: bool, cx: &mut Context<Self>) -> bool {
        if !self.find.text(cx).trim().is_empty() {
            return false;
        }
        let ids: Vec<String> = self.all.iter().map(|section| section.id.clone()).collect();
        if let Some(next) = step(&ids, &self.current, down) {
            self.go(next, cx);
        }
        true
    }

    /// The section the sheet shows, redrawn from the top.
    fn go(&mut self, id: String, cx: &mut Context<Self>) {
        self.current = id;
        self.body.set_offset(Default::default());
        if let Some(at) = self.all.iter().position(|section| section.id == self.current) {
            self.nav.scroll_to_item(at);
        }
        cx.notify();
    }

    /// A press on a section clears the search, shows it, and gives the keys back to the sheet.
    fn pick(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        self.find.put("", window, cx);
        self.go(id, cx);
        window.focus(&self.close, cx);
    }

    /// The Down or Up of the sheet's layer, which the walk hears while the keys are not in the field.
    pub fn move_section(&mut self, down: bool, cx: &mut Context<Self>) -> bool {
        self.step(down, cx)
    }

    /// The close button, which the sheet's keys rest on.
    pub fn close_focus(&self) -> &FocusHandle {
        &self.close
    }
}

impl Render for SheetView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        let keymap = self.heard.read(cx).keymap.clone();
        let leader = leader(&keymap);
        let query = self.find.text(cx).to_string();
        let found = matching(&self.all, &query);
        let filtering = !query.trim().is_empty();
        let nav = found.iter().map(|section| {
            let on = !filtering && section.id == self.current;
            let (id, at) = (section.id.clone(), self.all.iter().position(|each| each.id == section.id).unwrap_or(0));
            selected(
                h_flex()
                    .id(("sheet-item", at))
                    .role(Role::Button)
                    .test_support()
                    .aria_label(SharedString::from(section.title.clone()))
                    .aria_selected(on)
                    .flex_shrink_0()
                    .h(rem(tokens.control_h))
                    .px(rem(tokens.sp_8))
                    .gap(rem(tokens.sp_8))
                    .items_center()
                    .rounded(rem(tokens.r_md))
                    .cursor_pointer()
                    .weighted(tokens.weight_medium)
                    .text_color(if on { tokens.fg } else { tokens.fg_muted })
                    .hover(move |style| style.text_color(tokens.fg))
                    .on_click(cx.listener(move |this, _, window, cx| this.pick(id.clone(), window, cx)))
                    .child(div().flex_1().min_w_0().truncate().child(SharedString::from(section.title.clone())))
                    .children((section.id == self.here).then(|| div().typed(tokens.type_small).weighted(tokens.weight_regular).text_color(tokens.fg_faint).child("here"))),
                on,
                &tokens,
            )
        });
        let shown: Vec<_> = if filtering {
            found.iter().enumerate().map(|(at, section)| body::section(at, section, true, &leader, &tokens).into_any_element()).collect()
        } else {
            self.all
                .iter()
                .find(|section| section.id == self.current)
                .or(self.all.first())
                .map(|section| body::section(0, section, true, &leader, &tokens).into_any_element())
                .into_iter()
                .collect()
        };
        let mono = SharedString::from(tokens.font_mono);
        let cap = |key: &'static str| keycap(key, mono.clone(), &tokens);
        v_flex()
            .id("sheet")
            .role(Role::Dialog)
            .test_support()
            .aria_label("Keyboard shortcuts")
            .w_full()
            .h(pixels(tokens.sheet_h, window.rem_size()).min(window.viewport_size().height * tokens.sheet_max_share))
            .child(
                div().border_b_1().border_color(tokens.line).child(head("sheet", "Keyboard shortcuts", move |window, cx| {
                    Overlays::get(cx).close(Overlay::Sheet, window, cx);
                }, Some(&self.close), &tokens)),
            )
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(
                        v_flex()
                            .flex_shrink_0()
                            .h_full()
                            .min_h_0()
                            .overflow_hidden()
                            .w(rem(tokens.sheet_side_w))
                            .gap(rem(tokens.sp_8))
                            .px(rem(tokens.sp_8))
                            .py(rem(tokens.sp_12))
                            .border_r_1()
                            .border_color(tokens.line)
                            .child(
                                h_flex()
                                    .flex_shrink_0()
                                    .h(rem(tokens.control_h))
                                    .px(rem(tokens.sp_8))
                                    .gap(rem(tokens.sp_8))
                                    .items_center()
                                    .border_1()
                                    .border_color(tokens.line_strong)
                                    .rounded(rem(tokens.r_md))
                                    .text_color(tokens.fg_muted)
                                    .child(Glyph::Search.at(tokens.icon_sm))
                                    .child(
                                        div().flex_1().min_w_0().text_color(tokens.fg).child(
                                            Input::new(self.find.input())
                                                .id("sheet-find")
                                                .appearance(false)
                                                .bordered(false)
                                                .focus_bordered(false)
                                                .aria_label("Find a key or an action"),
                                        ),
                                    ),
                            )
                            .child(
                                v_flex()
                                    .id("sheet-nav")
                                    .role(Role::Navigation)
                                    .aria_label("Sections")
                                    .flex_1()
                                    .min_h_0()
                                    .overflow_y_scroll()
                                    .track_scroll(&self.nav)
                                    .gap(rem(tokens.sp_2))
                                    .children(nav),
                            ),
                    )
                    .child(
                        v_flex()
                            .id("sheet-body")
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .min_h_0()
                            .overflow_y_scroll()
                            .track_scroll(&self.body)
                            .gap(rem(tokens.sp_24))
                            .pt(rem(tokens.sp_20))
                            .px(rem(tokens.sp_24))
                            .pb(rem(tokens.sp_24))
                            .children(if found.is_empty() {
                                vec![div().id("sheet-none").test_support().aria_label("No key or action matches.").py(rem(tokens.sp_12)).typed(tokens.type_small).text_color(tokens.fg_muted).child("No key or action matches.").into_any_element()]
                            } else {
                                shown
                            }),
                    ),
            )
            .child(
                foot(&tokens)
                    .gap(rem(tokens.sp_6))
                    .typed(tokens.type_small)
                    .text_color(tokens.fg_muted)
                    .child(cap("?"))
                    .child("opens on the section for where you are \u{b7}")
                    .child(cap("\u{2191}"))
                    .child(cap("\u{2193}"))
                    .child("change section \u{b7}")
                    .child(cap("Esc"))
                    .child("close"),
            )
    }
}

#[cfg(test)]
mod tests;
