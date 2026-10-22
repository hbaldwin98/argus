//! The Git stage: Changes and History under one header, drawn where the
//! Workspace's pane is while the session has a review or a history open.
//!
//! Core has no Git stage (`review::entry` is how the window enters and leaves
//! it), so this draws what the session says: the review as the Changes tab,
//! the history as the History tab, and nothing of either before its event. The
//! header names the branch, switches the tab, counts the files and has the
//! refresh and the close; each sends a call, and the stage changes when the
//! session's event says it did.
//!
//! The stage holds the keys while it is up: the keys of the open tab are heard
//! here when no field has them (`review::keys`; `/` takes the history's filter field), every other key is held and
//! not typed into the pane under it, and a field in the stage (the jump field,
//! the comment box) hears only its own. The stage takes the keys as it appears
//! and the rail gives them back to the pane when it goes.

use argus_desktop_core::connection::page::Call;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme as _};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, px, Context, Entity, FocusHandle, KeyDownEvent, Role, SharedString, Subscription, Window};

use crate::chrome::icon_button::icon_button;
use crate::connection::caller::Caller;
use crate::icons::Glyph;
use crate::style::Typed as _;
use crate::tokens::{fs, radius, size, sp, ty};
use crate::connection::heard::Heard;
use crate::keys::page_key;
use crate::review::changes::Changes;
use crate::review::filter::Filter;
use crate::review::history::History;
use crate::review::entry::{checkout_here, Tab};
use crate::review::keys::{self, Press};
use crate::review::segment;
use crate::theme::Ledger;

pub struct GitStage {
    heard: Entity<Heard>,
    caller: Caller,
    changes: Entity<Changes>,
    history: Entity<History>,
    /// The history's filter field, which sits in the strip while History is the tab.
    filter: Entity<Filter>,
    focus: FocusHandle,
    /// Whether the stage was up at the last update, to take the keys once as it appears.
    was_up: bool,
    /// Whether a field of the stage (the jump field, the comment box) was open
    /// at the last update, to give the keys back to the stage when it shuts.
    had_field: bool,
    /// Whether the session had a history open at the last update, to give the
    /// keys back to the stage when it goes with the filter field holding them.
    had_history: bool,
    _subscriptions: [Subscription; 1],
}

impl GitStage {
    pub fn new(heard: Entity<Heard>, caller: Caller, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let changes = cx.new(|cx| Changes::new(heard.clone(), caller.clone(), window, cx));
        let history = cx.new(|cx| History::new(heard.clone(), caller.clone(), cx));
        let focus = cx.focus_handle();
        let filter = cx.new(|cx| Filter::new(heard.clone(), caller.clone(), focus.clone(), window, cx));
        let following = cx.observe_in(&heard, window, |this, _, window, cx| this.follow(window, cx));
        let mut stage = Self {
            heard,
            caller,
            changes,
            history,
            filter,
            focus,
            was_up: false,
            had_field: false,
            had_history: false,
            _subscriptions: [following],
        };
        // A window opened on a session that already has a review hears nothing
        // more to follow it by, so it follows what it was given: the diff is
        // handed its rows here rather than staying blank until the next event.
        stage.follow(window, cx);
        stage
    }

    /// Follows the session. The stage takes the keys as it appears, then the
    /// tab follows its review, which may hand them to an open field.
    fn follow(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (up, review, history) = {
            let heard = self.heard.read(cx);
            (heard.git_up(), heard.review.clone(), heard.history.clone())
        };
        if up && !self.was_up {
            window.focus(&self.focus, cx);
        }
        self.was_up = up;
        let field = review.as_ref().is_some_and(|page| page.comment.is_some() || page.jump.is_some());
        if let Some(review) = review {
            self.changes.update(cx, |changes, cx| changes.follow(&review, window, cx));
        }
        let historical = history.is_some();
        let filtering = history.as_ref().map(|page| page.filter.as_str());
        self.filter.update(cx, |filter, cx| filter.follow(filtering, window, cx));
        if let Some(history) = history {
            self.history.update(cx, |list, cx| list.follow(&history, cx));
        }
        // A history that goes while its filter held the keys leaves them on nothing.
        if self.had_history && !historical && up {
            window.focus(&self.focus, cx);
        }
        self.had_history = historical;
        // A field that shuts leaves the keys on nothing: the stage has them back,
        // so the review's own keys are heard again (CROSS-11).
        if self.had_field && !field && up {
            window.focus(&self.focus, cx);
        }
        self.had_field = field;
        cx.notify();
    }

    fn ask(&self, call: Call, cx: &Context<Self>) {
        if !self.heard.read(cx).held() {
            self.caller.call(call);
        }
    }

    /// A tab pressed: the other one's close, then this one's open.
    fn show(&mut self, tab: Tab, cx: &mut Context<Self>) {
        if self.heard.read(cx).held() {
            return;
        }
        let calls = self.heard.update(cx, |heard, cx| {
            let (open, here) = (heard.open(), checkout_here(&heard.rail, None));
            let calls = heard.git.show(tab, open, here);
            cx.notify();
            calls
        });
        for call in calls {
            self.caller.call(call);
        }
    }

    /// The stage shut: the close of whatever the session has open.
    fn close(&mut self, cx: &mut Context<Self>) {
        if self.heard.read(cx).held() {
            return;
        }
        let calls = self.heard.update(cx, |heard, cx| {
            let open = heard.open();
            let calls = heard.git.leave(open);
            cx.notify();
            calls
        });
        for call in calls {
            self.caller.call(call);
        }
    }

    /// A key while no field of the stage has the keys: the review's, or held.
    fn press(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        // A field's keys are its own: the press reaches here from it too.
        if !self.focus.is_focused(window) {
            return;
        }
        cx.stop_propagation();
        let (tab, reviewing, historical) = {
            let heard = self.heard.read(cx);
            (heard.git.tab(heard.open()), heard.review.is_some(), heard.history.is_some())
        };
        // Left and Right are no key of the review's or the history's: they
        // scroll the diff on screen sideways, as the page's native box does.
        let sideways = match event.keystroke.key.as_str() {
            "left" => -4.0,
            "right" => 4.0,
            _ => 0.0,
        };
        if sideways != 0.0 && !event.keystroke.modifiers.modified() {
            let diff = match tab {
                Tab::Changes => self.changes.read(cx).diff().clone(),
                Tab::History => self.history.read(cx).diff().clone(),
            };
            diff.update(cx, |diff, cx| diff.nudge(sideways, cx));
            return;
        }
        // Ctrl+C (Cmd+C) copies what a drag selected in the diff on screen;
        // with nothing selected it is no key of the review's.
        let keystroke = &event.keystroke;
        if keystroke.key == "c" && (keystroke.modifiers.control || keystroke.modifiers.platform) && !keystroke.modifiers.alt {
            let copied = match tab {
                Tab::Changes => self.changes.update(cx, |changes, cx| changes.copy(cx)),
                Tab::History => self.history.update(cx, |history, cx| history.copy(cx)),
            };
            if copied {
                return;
            }
        }
        let Some(key) = page_key::from_keystroke(&event.keystroke) else { return };
        // Each tab hears its own keys once the session has it open; before that
        // only Escape is heard, which shuts the stage as it does the tab.
        let pressed = match tab {
            Tab::Changes if reviewing => keys::review(&key),
            Tab::History if historical => keys::history(&key),
            _ => keys::review(&key).filter(|press| matches!(press, Press::Close)),
        };
        match pressed {
            Some(Press::Ask(call)) => self.ask(call, cx),
            Some(Press::Close) => self.close(cx),
            // `/`: the page's own, nothing is sent (SHARED-9).
            Some(Press::Field) => self.filter.update(cx, |filter, cx| filter.focus(window, cx)),
            None => {}
        }
    }

    fn tab(&self, id: &'static str, label: &'static str, count: Option<usize>, tab: Tab, on: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let ledger = *Ledger::global(cx);
        segment::pill(id, Role::Tab, on, &ledger)
            .child(label)
            .when_some(count, |pill, n| pill.child(segment::count(n.to_string(), &ledger)))
            .on_click(cx.listener(move |this, _, window, cx| {
                // A pill takes focus when clicked, and the tab's keys are heard
                // only on the stage itself.
                window.focus(&this.focus, cx);
                this.show(tab, cx)
            }))
    }

    /// The strip above the tab: the branch, the tabs, the files' count and the
    /// two buttons.
    fn strip(&self, tab: Tab, ledger: &Ledger, cx: &mut Context<Self>) -> impl IntoElement {
        let heard = self.heard.read(cx);
        let (review, history) = (heard.review.clone(), heard.history.clone());
        let branch = match tab {
            Tab::Changes => review.as_ref().map(|page| (page.branch.clone(), page.name.clone())),
            Tab::History => history.as_ref().map(|page| (page.branch.clone(), page.name.clone())),
        };
        let files = review.as_ref().map(|page| page.files.len());
        let changes_on = tab == Tab::Changes;
        let mono = cx.theme().mono_font_family.clone();
        let close_label = if changes_on { "Close the review" } else { "Close the history" };
        h_flex()
            .id("git-strip")
            .flex_shrink_0()
            .h(px(sp::S44))
            .px(px(sp::S24))
            .gap(px(sp::S12))
            .items_center()
            .border_b_1()
            .border_color(ledger.line)
            // `.chip-branch`: the branch once, as a pill, in the reference colour.
            .when_some(branch, |strip, (branch, name)| {
                strip.child(
                    h_flex()
                        .id("git-branch")
                        .test_support()
                        .aria_label(SharedString::from(name.clone()))
                        .tooltip(move |window, cx| Tooltip::new(name.clone()).build(window, cx))
                        .flex_shrink_0()
                        .h(px(size::ICON_BTN))
                        .px(px(sp::S8))
                        .gap(px(sp::S6))
                        .items_center()
                        .rounded(px(radius::PILL))
                        .border_1()
                        .border_color(ledger.line_strong)
                        .text_color(ledger.link)
                        .font_family(mono)
                        .text_size(px(fs::MD))
                        .child(Glyph::Branch.at(14.0))
                        .child(SharedString::from(branch)),
                )
            })
            .child(
                segment::track("git-tabs", "Git view", Role::TabList, ledger)
                    .child(self.tab("git-tab-changes", "Changes", files, Tab::Changes, changes_on, cx))
                    .child(self.tab("git-tab-history", "History", None, Tab::History, !changes_on, cx)),
            )
            // `.meta`: what the history lists, beside the tabs as the page's head says it.
            .when(!changes_on && history.is_some(), |strip| {
                strip.child(div().typed(ty::SMALL).text_color(ledger.muted).child("newest 100 on HEAD"))
            })
            .child(div().flex_1())
            .when(!changes_on && history.is_some(), |strip| strip.child(self.filter.clone()))
            .when_some(review.as_ref().filter(|page| changes_on && !page.files.is_empty()), |strip, page| {
                let plural = if page.files.len() == 1 { "" } else { "s" };
                strip.child(
                    h_flex()
                        .id("review-count")
                        .test_support()
                        .aria_label(SharedString::from(format!(
                            "{} file{plural} \u{b7} +{} \u{2212}{}",
                            page.files.len(),
                            page.added,
                            page.removed
                        )))
                        .gap(px(sp::S4))
                        .typed(ty::SMALL)
                        .text_color(ledger.muted)
                        .child(SharedString::from(format!("{} file{plural} \u{b7}", page.files.len())))
                        .child(div().text_color(ledger.success).child(SharedString::from(format!("+{}", page.added))))
                        .child(div().text_color(ledger.danger).child(SharedString::from(format!("\u{2212}{}", page.removed)))),
                )
            })
            .when(changes_on && review.is_some(), |strip| {
                strip.child(
                    icon_button("review-refresh", "Refresh", false, ledger)
                        .tooltip(|window, cx| Tooltip::new("Refresh").build(window, cx))
                        .child(Glyph::Refresh.at(14.0))
                        .on_click(cx.listener(|this, _, window, cx| {
                            // A button takes focus when clicked, and the review's keys
                            // are heard only on the stage itself.
                            window.focus(&this.focus, cx);
                            this.ask(Call::ReviewRefresh, cx)
                        })),
                )
            })
            .child(
                icon_button("git-close", close_label, false, ledger)
                    .tooltip(move |window, cx| Tooltip::new(close_label).build(window, cx))
                    .child(Glyph::Close.at(14.0))
                    .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
            )
    }

    /// A quiet line standing where a body is not drawn.
    fn said(id: &'static str, words: &'static str, ledger: &Ledger) -> gpui_kit::AnyElement {
        div()
            .id(id)
            .test_support()
            .aria_label(SharedString::from(words))
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .text_color(ledger.faint)
            .child(words)
            .into_any_element()
    }
}

impl Render for GitStage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ledger = *Ledger::global(cx);
        let (tab, reviewing, historical, held) = {
            let heard = self.heard.read(cx);
            (heard.git.tab(heard.open()), heard.review.is_some(), heard.history.is_some(), heard.held())
        };
        // What the session says is open is drawn; with nothing open the stage
        // is up on the window's word alone, saying what to do.
        let body = match tab {
            Tab::Changes if reviewing => self.changes.clone().into_any_element(),
            Tab::History if historical => self.history.clone().into_any_element(),
            _ => Self::said("git-pick", "Pick a checkout in the sidebar, then press Git.", &ledger),
        };
        v_flex()
            .id("git")
            .test_support()
            .key_context("Git")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| this.press(event, window, cx)))
            .size_full()
            // The dialog's surface: the files and the commits sit on it, the diff
            // is sunk below it.
            .bg(ledger.raised)
            .text_color(ledger.muted)
            .when(held,|stage| stage.opacity(0.5))
            .child(self.strip(tab, &ledger, cx))
            .child(div().id("git-body").flex_1().min_h_0().child(body))
    }
}

#[cfg(test)]
mod tests;
