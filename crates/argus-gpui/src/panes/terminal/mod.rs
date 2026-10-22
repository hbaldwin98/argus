//! A live pane's screen: the Workspace's on the stage, or one of the console's two sides, or a
//! split cell's. What it draws of the pane its side shows, who has the keys, the keys it sends,
//! and the room it reports.
//!
//! It draws `Heard`'s screen of its side only while that screen is of the pane the side shows (the
//! rail's shown pane, or the console's), so the old pane's rows never sit under the new one's
//! header; until the first frame of a newly picked pane arrives it is empty, and after 250 ms of
//! waiting the page's skeleton bars stand in for it (`skeleton`). It repaints when `Heard` says a
//! frame changed its side's screen, never on a timer, and reads the palette again when the theme
//! is installed.
//!
//! The keys are the session's: a `Call::Key` goes to whichever pane it says has them, whatever
//! screen GPUI's focus is on, so the console's screens take GPUI's focus only when the console
//! says they are typed in. A press on a console screen that is not typed in asks for the keys
//! (`console{select}`) and waits for the answer, as the page's does. The screen is a field of the
//! key walk (`keys/field`): while it has the keys every keystroke the page can name is a
//! `Call::Key`, and what the leader, the paste chord and the child's keyboard mode make of it is
//! the session's. A key the walk does not take (a bare modifier, an input method's) goes on to
//! GPUI's input handler (`keys/text`), which sends the committed text. On a Mac the command key's
//! chords that the Pane place does not bind are the window's and are not sent.
//!
//! The mouse, the selection and the wheel are `pointer`'s; the stage header's size label is
//! [`Labelled`], told when its words change.

pub mod label;
pub mod pointer;
pub mod room;
pub mod skeleton;

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use argus_desktop_core::connection::page::{Call, Side};
use argus_desktop_core::keys::keymap::leaves_to_window;
use argus_desktop_core::panes::console::ConsoleGesture;
use argus_desktop_core::panes::pointer::{Pointer, Wheel};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{
    div, point, prelude::*, px, App, Context, ElementId, Entity, EventEmitter, FocusHandle, Pixels, Point, Subscription, Task, Window,
};

use crate::connection::caller::Caller;
use crate::connection::heard::{Heard, Painted};
use crate::kit::ghosts::ghosts;
use crate::keys::field::{Fields, Registered};
use crate::keys::layer::Verdict;
use crate::panes::element::TerminalElement;
use crate::kit::fonts;
use crate::panes::palette::Palette;
use crate::panes::screen::{Cursor, Screen};
use crate::panes::size_label::SizeLabel;
use crate::theme::scale::rem;
use crate::theme::Tokens;
use skeleton::Skeleton;

/// The words the stage header says of the Workspace's grid changed (`panes/size_label`).
pub struct Labelled(pub String);

pub struct Terminal {
    side: Side,
    heard: Entity<Heard>,
    caller: Caller,
    focus: FocusHandle,
    palette: Palette,
    /// Stood in for a screen that is not of the pane on screen.
    blank: Arc<Screen>,
    /// The grid the element last said its size held.
    seen: Rc<Cell<Option<(u16, u16)>>>,
    pub(crate) cell: Rc<Cell<(Pixels, Pixels)>>,
    /// Where the element's grid starts in the window, for the cell under the pointer.
    origin: Rc<Cell<Point<Pixels>>>,
    paints: Rc<Cell<u64>>,
    /// The room the element measured, waiting out [`room::SETTLE`].
    wanted: Option<(u16, u16)>,
    /// The room last reported to the session, which is told only a change.
    sent: Option<(u16, u16)>,
    /// A settled size was not sent because a sash was held; sent when it is let go.
    waiting: bool,
    settling: Option<Task<()>>,
    pointer: Pointer,
    wheel: Wheel,
    /// Whether the command key's chords are the window's: on a Mac. A cell so a test can be one;
    /// the key walk's closure holds the other end.
    #[cfg_attr(not(test), allow(dead_code))]
    command_is_window: Rc<Cell<bool>>,
    skeleton: Skeleton,
    /// What the header says of the grid, kept for the Workspace's screen only.
    label: SizeLabel,
    labelled: String,
    relabelling: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
    /// The screen's place in the key walk, ended with the view.
    _field: Registered,
}

impl EventEmitter<Labelled> for Terminal {}

impl Terminal {
    pub fn new(heard: Entity<Heard>, caller: Caller, side: Side, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        let workspace = side == Side::Workspace;
        // A split cell or a view has a screen from now until this goes.
        heard.update(cx, |heard, _| heard.screens.bind(side));
        let subscriptions = vec![
            // A frame repaints; the rail moving the shown pane re-picks what is drawn.
            cx.subscribe(&heard, move |this, _, painted: &Painted, cx| {
                if painted.0 == side {
                    this.framed(cx);
                    cx.notify();
                }
            }),
            cx.observe(&heard, |this, _, cx| {
                // A sash let go: the size it settled on is the one to report.
                if this.waiting {
                    this.settle(cx);
                }
                this.expect(cx);
                cx.notify();
            }),
            // The cursor is hollow without the keys or the window.
            cx.on_focus_in(&focus, window, |_, _, cx| cx.notify()),
            cx.on_focus_out(&focus, window, |_, _, _, cx| cx.notify()),
            cx.observe_window_activation(window, move |this, window, cx| {
                // One report for the window, not one per screen.
                if workspace {
                    this.caller.call(Call::Focus { focused: window.is_window_active() });
                }
                cx.notify();
            }),
            // A theme installed over this one: the sixteen and the ground are read again, and the
            // room is not (a family change is the font's, not the theme's).
            cx.observe_global::<Tokens>(|this, cx| {
                this.palette = Palette::of(Tokens::global(cx));
                cx.notify();
            }),
        ];
        // The screen of a split cell or a view goes with the view, and so does a selection on it.
        cx.on_release(move |this, cx| {
            this.heard.update(cx, |heard, cx| {
                if heard.screens.unbind(side) {
                    cx.emit(Painted(side));
                }
            });
        })
        .detach();
        // The pane is what the person types into until they pick another place.
        if workspace {
            window.focus(&focus, cx);
        }
        // Every key the walk gives a focused screen goes to the session as the page sends it.
        let keys = caller.clone();
        let marks = heard.clone();
        let command_is_window = Rc::new(Cell::new(cfg!(target_os = "macos")));
        let mac = command_is_window.clone();
        let field = Fields::of(cx).register(window, focus.clone(), move |press, _, cx| {
            // The command key's chords are the window's (quit, close), but for those the Pane place
            // binds; the window goes on with the key.
            if leaves_to_window(press.page, mac.get()) {
                return Verdict::Held;
            }
            // Every key press clears every screen's selection (SHELL-INPUT-2).
            marks.update(cx, |heard, cx| heard.mark(None, cx));
            keys.call(Call::Key { key: press.page.clone() });
            Verdict::Taken
        });
        let mut this = Self {
            side,
            heard,
            caller,
            focus,
            palette: Palette::of(Tokens::global(cx)),
            blank: Arc::new(Screen::new()),
            seen: Rc::default(),
            cell: Rc::new(Cell::new((px(0.0), px(0.0)))),
            origin: Rc::new(Cell::new(point(px(0.0), px(0.0)))),
            paints: Rc::default(),
            wanted: None,
            sent: None,
            waiting: false,
            settling: None,
            pointer: Pointer::default(),
            wheel: Wheel::default(),
            command_is_window,
            skeleton: Skeleton::default(),
            label: SizeLabel::default(),
            labelled: String::new(),
            relabelling: None,
            _subscriptions: subscriptions,
            _field: field,
        };
        this.expect(cx);
        this
    }

    pub fn focus_handle(&self) -> &FocusHandle {
        &self.focus
    }

    /// How many times the screen was painted.
    pub fn paints(&self) -> u64 {
        self.paints.get()
    }

    /// The palette the screen is drawn in, read from the theme installed.
    pub fn palette(&self) -> &Palette {
        &self.palette
    }

    /// What the stage header says of the grid, while the pane the screen holds is the one on
    /// screen: nothing in between, when the screen still holds the old pane's rows.
    pub fn size_label(&self, cx: &App) -> &str {
        let heard = self.heard.read(cx);
        let shown = heard.pane_of(self.side);
        if shown.is_some() && heard.screen_of(self.side).pane() == shown {
            &self.labelled
        } else {
            ""
        }
    }

    /// Whether this side has the keys, which its cursor shows and which a move over it reports to
    /// the child for. The session says it: the console's `typing`, or for the grid's cells the
    /// cell the rail numbers as keyed (`Heard::cell_has_keys`).
    fn has_keys(&self, heard: &Heard) -> bool {
        let pane = heard.pane_of(self.side);
        match self.side {
            Side::Workspace | Side::Beside(_) => heard.cell_has_keys(self.side),
            Side::First | Side::Second => heard.keyed(pane),
            Side::View(_) => heard.view_console(self.side).is_some_and(|console| console.keyed),
        }
    }

    /// A press on the screen hands its pane the keys, as the page's does (SHELL-INPUT-5). One that
    /// has them takes GPUI's focus. The Workspace's asks for them with `select`, a split cell or a
    /// view with `focus_view`, a console's side with `console{select}`, and waits: the focus
    /// moves when the `Console` event says so (`panes/console`), not on the press, and nothing is
    /// asked while the daemon is gone.
    fn claim(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let heard = self.heard.read(cx);
        let pane = heard.pane_of(self.side);
        if self.has_keys(heard) {
            window.focus(&self.focus, cx);
            return;
        }
        // GPUI hands a tracked element the keys on any press inside it, which
        // would move them before the session has said so.
        window.prevent_default();
        if let Some(pane) = pane.filter(|_| !heard.held()) {
            self.caller.call(claiming(self.side, pane));
        }
    }

    /// Text an input method committed.
    pub(crate) fn text(&mut self, text: &str) {
        if !text.is_empty() {
            self.caller.call(Call::Text { text: text.to_owned() });
        }
    }

    /// Text pasted through the platform's own paste, as `paste{text}`.
    pub(crate) fn paste(&mut self, text: String) {
        if !text.is_empty() {
            self.caller.call(Call::Paste { text });
        }
    }

    /// The cursor, and the size of one cell, for the input method's
    /// candidate window to sit by; nothing before the first paint measured
    /// a cell.
    pub(crate) fn cursor_cell(&self, cx: &App) -> Option<(Cursor, (Pixels, Pixels))> {
        let cell = self.cell.get();
        (cell.0 > px(0.0)).then(|| (self.heard.read(cx).screen_of(self.side).cursor(), cell))
    }
}

/// What a press on a screen asks of the session to give its pane the keys.
pub(crate) fn claiming(side: Side, pane: u64) -> Call {
    match side {
        Side::Workspace => Call::Select { pane },
        Side::Beside(_) | Side::View(_) => Call::FocusView { pane },
        Side::First | Side::Second => Call::Console { gesture: ConsoleGesture::Select { pane } },
    }
}

/// The screen to draw: `heard`'s when it is of the pane the rail says is
/// shown, else `blank`. Picking another pane moves `shown` before the new
/// pane's first frame arrives, and the old rows are not the new pane's.
fn drawn(heard: &Heard, side: Side, blank: &Arc<Screen>) -> Arc<Screen> {
    let shown = heard.pane_of(side);
    if shown.is_some() && heard.screen_of(side).pane() == shown {
        heard.screen_of(side).clone()
    } else {
        blank.clone()
    }
}

/// The element id of a side's screen.
fn id(side: Side) -> ElementId {
    match side {
        Side::First => "terminal-first".into(),
        Side::Second => "terminal-second".into(),
        Side::Beside(n) => ("terminal-beside", usize::from(n)).into(),
        Side::View(n) => ("terminal-view", usize::from(n)).into(),
        Side::Workspace => "terminal".into(),
    }
}

impl Render for Terminal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        let screen = drawn(self.heard.read(cx), self.side, &self.blank);
        let focused = self.focus.is_focused(window) && window.is_window_active();
        let selection = self.heard.read(cx).screens.selection_of(self.side);
        let element = TerminalElement {
            screen,
            palette: self.palette.clone(),
            family: cx.theme().mono_font_family.clone(),
            behind: fonts::mono_behind(tokens.font_mono_bundled),
            focused,
            selection,
            share: tokens.share_term_selection,
            terminal: cx.entity(),
            focus: self.focus.clone(),
            seen: self.seen.clone(),
            cell: self.cell.clone(),
            origin: self.origin.clone(),
            paints: self.paints.clone(),
        };
        div()
            .id(id(self.side))
            .test_support()
            .key_context("Terminal")
            .track_focus(&self.focus)
            .relative()
            .size_full()
            .bg(tokens.see_sunken)
            .p(rem(tokens.sp_8))
            // A press with any button on the pane gives it the keys, or asks for them
            // (SHELL-INPUT-5, -9); only the start of a selection is the left button's.
            .on_any_mouse_down(cx.listener(|this, event, window, cx| this.pressed(event, window, cx)))
            .on_mouse_move(cx.listener(|this, event, _, cx| this.hovered(event, cx)))
            .on_scroll_wheel(cx.listener(|this, event, _, cx| this.scrolled(event, cx)))
            .child(pointer::watch(cx.weak_entity()))
            .child(element)
            // Bars stand in for a pane until its first screen arrives.
            .when(self.skeleton.shown(), |this| {
                this.child(div().absolute().inset_0().pt(rem(tokens.sp_4)).child(ghosts(
                    "terminal-skeleton",
                    skeleton::BARS,
                    &tokens,
                )))
            })
    }
}

#[cfg(test)]
mod tests;
