//! The icon strip down the rail's left edge: the stages as icons with a count where the window
//! knows one, then the settings gear and the server dot at its foot.
//!
//! (`project/Blend.dc.html`.) The lit stage is a filled tile with a 2px accent edge; Workspace's
//! badge is the attention colour and counts the panes that need the person, the others are neutral
//! and `counts` says which have a number to show. The keys the keymap names for a stage are in its
//! tooltip, not drawn beside it. The Extensions and Views entries the nav has are icons here too,
//! and open their list in the panel (`tail`). A folded rail is this strip alone: it takes Search
//! and New, which the panel's header holds otherwise, and the pane tiles under the stages.

use argus_desktop_core::connection::page::Call;
use argus_desktop_core::keys::keymap::Action;
use argus_desktop_core::layout::docks::Panel;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, AnyElement, Context, MouseButton, MouseDownEvent, Role, SharedString, TestSupportExt as _, WindowControlArea};

use super::counts::Counts;
use super::row::Ctx;
use super::slice::Slice;
use super::RailView;
use crate::chrome::nav::Nav;
use crate::keys::over::run_over;
use crate::kit::icon_button::Node;
use crate::kit::icons::Glyph;
use crate::kit::tip::tip;
use crate::layout::carry::Carrier;
use crate::layout::work::FromRail;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// The strip's width, and the tile of a stage in it (`.ab`, 36px).
pub(super) const STRIP_W: f32 = 48.0;
const TILE: f32 = 36.0;
const BADGE_H: f32 = 14.0;

/// What a stage tile says: its mark, whether it is lit, its badge, and its words.
pub(super) struct Tile {
    pub glyph: Glyph,
    pub lit: bool,
    /// The count and whether it asks for the person (the attention colour).
    pub badge: Option<(usize, bool)>,
    /// What a reader that cannot see the badge hears.
    pub said: String,
    pub title: String,
}

/// `.ab`: a 36px tile of an icon, muted until the pointer is on it, lit while its stage is up.
pub(super) fn tile(id: &'static str, tile: Tile, tokens: &Tokens) -> Node {
    let Tile { glyph, lit, badge, said, title } = tile;
    let (fill, text) = (tokens.fill, tokens.fg);
    h_flex()
        .id(id)
        .role(Role::Button)
        .test_support()
        .aria_label(said)
        .aria_selected(lit)
        .tooltip(tip(title))
        .relative()
        .size(rem(TILE))
        .flex_none()
        .items_center()
        .justify_center()
        .overflow_hidden()
        .rounded(rem(8.0))
        .cursor_pointer()
        .text_color(if lit { text } else { tokens.fg_muted })
        .when(lit, |tile| tile.bg(tokens.bg_selected))
        .when(!lit, |tile| tile.hover(move |style| style.bg(fill).text_color(text)))
        .child(glyph.at(18.0))
        // The 2px accent edge on the lit tile's left.
        .when(lit, |tile| tile.child(div().absolute().left_0().top_0().bottom_0().w(rem(tokens.rule)).bg(tokens.accent)))
        .children(badge.map(|(count, loud)| {
            div()
                .id(SharedString::from(format!("{id}-badge")))
                .test_support()
                .absolute()
                .top(rem(3.0))
                .right(rem(2.0))
                .min_w(rem(BADGE_H))
                .h(rem(BADGE_H))
                .px(rem(3.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(rem(BADGE_H / 2.0))
                .bg(if loud { tokens.attention } else { tokens.bg_raised })
                .text_color(if loud { tokens.bg_sunken } else { tokens.fg })
                .text_size(rem(tokens.fs_2xs))
                .weighted(tokens.weight_semibold)
                .child(count.to_string())
        }))
}

/// The stages, in the strip's order: the person's way between them. Search and New are the panel
/// header's, except in a folded rail, which has no panel.
fn stages(icons: bool) -> impl Iterator<Item = Nav> {
    Nav::ALL.into_iter().filter(move |nav| icons || !matches!(nav, Nav::Search | Nav::New))
}

impl RailView {
    /// The strip: the logo, the stages, and at its foot the gear and the server dot. `middle` is
    /// what fills the room between (a folded rail's pane tiles), else a spacer.
    pub(super) fn strip(&self, slice: &Slice, ctx: Ctx, middle: Option<AnyElement>, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *tokens;
        let drag = |area: gpui_kit::Div| {
            area.window_control_area(WindowControlArea::Drag)
                .when(!cfg!(windows), |this| this.on_mouse_down(MouseButton::Left, |_, window, _| crate::window::drag::start_move(window)))
        };
        // The strip runs to the window's top edge: its logo cell is the title bar's left end and
        // moves the window.
        let logo = drag(h_flex().h(rem(tokens.sp_44)).w_full().flex_none().mb(rem(6.0)).items_center().justify_center())
            .child(Glyph::Logo.at(tokens.icon_glyph + 4.0).text_color(tokens.accent));
        let mut stages_col = v_flex().flex_none().items_center().gap(rem(tokens.sp_4));
        if slice.furniture {
            for nav in stages(ctx.icons) {
                stages_col = stages_col.child(self.stage_tile(nav, slice, ctx, &tokens, cx));
            }
            if !ctx.icons {
                stages_col = stages_col.children(self.fold_tiles(slice, ctx, &tokens, cx));
            }
        }
        v_flex()
            .id("rail-strip")
            .test_support()
            .flex_none()
            .h_full()
            .when(ctx.icons, |strip| strip.w_full())
            .when(!ctx.icons, |strip| strip.w(rem(STRIP_W)).border_r_1().border_color(tokens.line))
            .items_center()
            .bg(tokens.see_sunken)
            .pb(rem(10.0))
            .gap(rem(tokens.sp_4))
            .child(logo)
            .child(stages_col)
            .child(middle.unwrap_or_else(|| div().flex_1().into_any_element()))
            .when(slice.furniture, |strip| strip.child(self.settings_tile(slice, &tokens)).child(self.server_dot(slice, ctx, &tokens, cx)))
    }

    fn stage_tile(&self, nav: Nav, slice: &Slice, ctx: Ctx, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let held = ctx.held;
        let at = Nav::ALL.iter().position(|each| *each == nav).unwrap_or_default();
        let counts: &Counts = &slice.counts;
        let count = counts.of_nav(nav);
        let said = count.map_or_else(|| nav.label().to_owned(), |count| counts.said(nav, count));
        let lit = nav.lit(slice.rail.stage, slice.git_up);
        let browsers_none = slice.browsers.is_empty();
        let tile = tile(
            nav.id(),
            Tile { glyph: nav.glyph(), lit, badge: count.map(|count| (count, nav == Nav::Workspace)), said, title: slice.titles.get(at).cloned().unwrap_or_default() },
            tokens,
        )
        .on_click(cx.listener(move |this, _, window, cx| {
            // Search is the palette, the window's own as Ctrl K is: it opens it and the palette tells the
            // session, which never opens it on the window's behalf.
            if nav == Nav::Search {
                if !held {
                    run_over(Action::Palette, window, cx);
                }
                return;
            }
            this.go(nav, held, cx)
        }));
        match nav {
            // A press that travels carries the changes beside an agent, opened for the checkout here
            // when no review is.
            Nav::Git => tile.on_mouse_down(MouseButton::Left, cx.listener(move |this, event: &MouseDownEvent, _, cx| this.carry_git(event, held, cx))).into_any_element(),
            // A press that travels carries every browser, as their panel, beside an agent; with none
            // open, a cell for the next.
            Nav::Browsers => tile
                .on_mouse_down(MouseButton::Left, move |event: &MouseDownEvent, _, cx| {
                    if !held {
                        let rail = if browsers_none { FromRail::Empty } else { FromRail::There };
                        Carrier::of(cx).press_rail(event.position, Panel::Browsers, nav.label().into(), rail, cx);
                    }
                })
                .into_any_element(),
            _ => tile.into_any_element(),
        }
    }

    /// The Extensions and Views entries: an icon each while they hold anything, lit while their
    /// list is open in the panel.
    fn fold_tiles(&self, slice: &Slice, ctx: Ctx, tokens: &Tokens, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let held = ctx.held;
        let mut tiles = Vec::new();
        if !slice.extensions.is_empty() {
            let count = slice.extensions.len();
            tiles.push(
                tile(
                    "nav-extensions",
                    Tile { glyph: Glyph::Puzzle, lit: slice.extensions_listed, badge: Some((count, false)), said: format!("Extensions, {count}"), title: "Extensions".into() },
                    tokens,
                )
                .aria_expanded(slice.extensions_listed)
                .on_click(cx.listener(move |this, _, _, _| this.ask(held, Call::ToggleExtensions)))
                .into_any_element(),
            );
        }
        if !slice.saved.is_empty() {
            let count = slice.saved.len();
            let listed = self.views.listed;
            tiles.push(
                tile("nav-views", Tile { glyph: Glyph::Box, lit: listed, badge: Some((count, false)), said: format!("Views, {count}"), title: "Views".into() }, tokens)
                    .aria_expanded(listed)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.views.listed = !this.views.listed;
                        cx.notify();
                    }))
                    .into_any_element(),
            );
        }
        tiles
    }

    /// Settings is the window's, not the daemon's: it opens while the daemon is gone.
    fn settings_tile(&self, slice: &Slice, tokens: &Tokens) -> impl IntoElement {
        tile("foot-settings", Tile { glyph: Glyph::Gear, lit: false, badge: None, said: "Settings".into(), title: slice.server.settings_title.clone() }, tokens).on_click(|_, window, cx| {
            run_over(Action::Settings, window, cx);
        })
    }
}
