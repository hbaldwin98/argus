//! The Feature stage's index: the repository's features as a list beside a peek or as a board,
//! under a head with the filter and over a bar of keys (`drawFeatures` of `feature_stage.ts`).
//!
//! Rust holds everything the index shows (`board::features`, `board::feature_stage`): which
//! features are listed and in what order, the fold, the filter, the layout, the cursor. This draws
//! what the last snapshot says and sends what is pressed, so a refused action leaves the index
//! showing what is true. Before the board arrives there are loading bars; with no repository a card
//! says so; with none of its features a card says how to make one.
//!
//! A feature opened from the index, and the note a new one is written as, are drawn in place of it
//! by their own surfaces; while either is up this draws nothing. The strip's words and tools are the
//! stage's offer to the header (`strip`), and its keys are the Feature layer of the key walk (`keys`).

pub mod bar;
pub mod empty;
pub mod head;
pub mod keys;
pub mod strip;

use std::cell::Cell;
use std::rc::Rc;

use argus_desktop_core::board::features::Layout;
use argus_desktop_core::board::index_words::layout_of;
use argus_desktop_core::connection::page::{Call, Features, Stage};
use argus_desktop_core::keys::keymap::{Action, Layer, Place};
use gpui_kit::component::input::InputEvent;
use gpui_kit::component::v_flex;
use gpui_kit::{div, prelude::*, App, Bounds, Context, Entity, FocusHandle, Focusable as _, Pixels, Subscription, TestSupportExt as _, Window};

use super::columns::Lanes;
use super::ctx::Ctx;
use super::list::IndexList;
use super::menus::Menu;
use crate::chrome::header::slots::{Offer, Slots};
use crate::chrome::stage::route::narrow;
use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::keys::field::{Fields, Registered};
use crate::keys::layer::Layers;
use crate::kit::field::Field;
use crate::layout::bodies::Contents;
use crate::theme::scale::css;
use crate::theme::Tokens;
use strip::StripTools;

pub struct FeatureStage {
    ctx: Ctx,
    filter: Field,
    /// What the session was last told the filter is, so a field put back to what the index says
    /// sends nothing.
    known: gpui_kit::SharedString,
    /// The strip's search button has opened the field, which a narrow window folds away.
    filtering: bool,
    /// The window is narrow: the field folds into the strip's button.
    narrow: bool,
    list: Entity<IndexList>,
    lanes: Entity<Lanes>,
    menu: Entity<Menu>,
    tools: Entity<StripTools>,
    /// Where the peeked row or card is, for the Move menu the `M` key opens beside it.
    anchor: Rc<Cell<Bounds<Pixels>>>,
    /// Where the keys rest while no field holds them: the stage itself.
    focus: FocusHandle,
    was_up: bool,
    _subscriptions: Vec<Subscription>,
    _field: Registered,
}

/// Builds the stage, and puts its strip, its keys and its body where the window looks for them.
pub fn install(heard: &Entity<Heard>, caller: &Caller, window: &mut Window, cx: &mut App) {
    let stage = cx.new(|cx| FeatureStage::new(heard.clone(), caller.clone(), window, cx));
    Contents::of(cx).offer_stage(Stage::Feature, stage.into());
}

impl FeatureStage {
    pub fn new(heard: Entity<Heard>, caller: Caller, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let ctx = Ctx { heard: heard.clone(), caller };
        let focus = cx.focus_handle();
        let filter = Field::new("Filter by title or #", window, cx);
        let anchor: Rc<Cell<Bounds<Pixels>>> = Rc::default();
        let menu = cx.new(|_| Menu::new(ctx.clone()));
        let list = cx.new(|_| IndexList::new(ctx.clone(), menu.clone(), anchor.clone()));
        let lanes = cx.new(|cx| Lanes::new(ctx.clone(), menu.clone(), anchor.clone(), cx));
        let this = cx.entity();
        let tools = cx.new(|_| StripTools::new(ctx.clone(), &this));
        let saying = heard.clone();
        let offered = tools.clone();
        Slots::of(cx).offer(
            Stage::Feature,
            Offer::default().says(move |cx| strip::says(saying.read(cx).features.as_deref())).tools(move |_, _| offered.clone().into_any_element()),
        );
        Layers::of(cx).register(Layer::Features, std::rc::Rc::new(keys::StageKeys { stage: cx.weak_entity() }));
        let typing = cx.subscribe(filter.input(), |this, input, event: &InputEvent, cx| match event {
            InputEvent::Change => {
                let text = input.read(cx).value();
                if text != this.known && !this.ctx.heard.read(cx).held() {
                    this.known = text.clone();
                    this.ctx.caller.call(Call::FilterFeatures { filter: text.to_string() });
                }
            }
            // Leaving the field empty puts the narrow window's field away again.
            InputEvent::Blur if input.read(cx).value().is_empty() && this.filtering => {
                this.filtering = false;
                cx.notify();
            }
            _ => {}
        });
        let following = cx.observe_in(&heard, window, |this, heard, window, cx| {
            let up = heard.read(cx).rail.stage == Stage::Feature;
            if let Some(said) = heard.read(cx).features.as_ref().map(|features| features.filter.clone()) {
                // Known first, so putting what the index says into the field is not an edit to send; a
                // field being typed in is left alone, and what was known stays.
                let before = std::mem::replace(&mut this.known, gpui_kit::SharedString::from(said.clone()));
                if !this.filter.show(&said, window, cx) && this.filter.text(cx) != said {
                    this.known = before;
                }
            }
            if up && !this.was_up {
                window.focus(&this.focus, cx);
            }
            this.was_up = up;
            cx.notify();
        });
        let back = focus.clone();
        let field = Fields::of(cx).register_place(window, filter.input().focus_handle(cx), Place::Filter, move |action, window, cx| {
            if action != Action::Leave {
                return false;
            }
            window.focus(&back, cx);
            true
        });
        Self {
            ctx,
            filter,
            known: Default::default(),
            filtering: false,
            narrow: false,
            list,
            lanes,
            menu,
            tools,
            anchor,
            focus,
            was_up: false,
            _subscriptions: vec![typing, following],
            _field: field,
        }
    }

    pub fn focus_handle(&self) -> &FocusHandle {
        &self.focus
    }

    /// `/`: the field takes the keys, shown first if a narrow window had folded it away. Nothing is sent.
    pub fn focus_filter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.filtering = true;
        cx.notify();
        let field = self.filter.clone();
        window.defer(cx, move |window, cx| field.focus(window, cx));
    }

    /// The strip's search button: opens the field, or puts it away.
    pub fn toggle_filter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.filtering {
            self.filtering = false;
            cx.notify();
        } else {
            self.focus_filter(window, cx);
        }
    }

    fn body(&self, features: &Features) -> gpui_kit::AnyElement {
        match layout_of(features) {
            Layout::List => self.list.clone().into_any_element(),
            Layout::Board => self.lanes.clone().into_any_element(),
        }
    }
}

impl Render for FeatureStage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        let narrow = narrow(css(window.viewport_size().width, window.rem_size()), tokens.narrow_below);
        if narrow != self.narrow {
            self.narrow = narrow;
            self.tools.update(cx, |_, cx| cx.notify());
        }
        let root = div().id("features").test_support().key_context("Features").track_focus(&self.focus).relative().size_full().flex().flex_col().text_color(tokens.fg);
        let features = self.ctx.features(cx);
        let Some(features) = features.filter(|features| !features.repository.is_empty()) else {
            return root.child(empty::none_chosen(&tokens)).into_any_element();
        };
        // An opened feature is the open-feature surface's, which draws in place of the index.
        if features.open.is_some() {
            return root.into_any_element();
        }
        let head = self.head_shown(&features).then(|| self.head(&features, &tokens));
        if !features.loaded {
            return root.children(head).child(empty::loading(&tokens)).into_any_element();
        }
        if features.active + features.queued + features.done + features.held + features.archived == 0 {
            return root.children(head).child(empty::nothing(&features.repository, &self.ctx, &tokens)).into_any_element();
        }
        root.children(head)
            .child(v_flex().flex_1().min_h_0().w_full().child(self.body(&features)))
            .child(bar::bar(features.layout, &tokens))
            .child(self.menu.clone())
            .into_any_element()
    }
}

#[cfg(test)]
mod tests;
