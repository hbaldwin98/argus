//! The Code stage: a checkout's files in a column that marks what changed, one file read whole in
//! the middle, and what the file defines on the right (`code/stage.ts`). Drawn where the
//! Workspace's pane is while the session's stage is Code.
//!
//! Rust holds the files, the cursor, the fields and the search (`argus-desktop-core::code`); this
//! draws the page it sends and says what is pressed or typed, so a refused action leaves the stage
//! showing what is true. The page is kept after the stage goes, scrolled where it was, for the
//! person coming back to it. Before the first page, or while it is not open, four loading bars
//! stand where the columns will be.
//!
//! The columns are three with a sash between each pair, which report their width once on release
//! and give the usual one back on a double-click; how wide each is, is `widths`. The files column
//! is `side`, the file area with no file read is `start`, the file read is `file`, and the keys are
//! the Code layer (`keys`). The outline lists what the file defines and changes (`outline`); the
//! editor, when it comes, stands in for the file's lines.

mod keys;
mod outline;
mod widths;

use std::cell::Cell;
use std::rc::Rc;

use argus_desktop_core::chrome::strip::Says;
use argus_desktop_core::code::start::{selected_match, start};
use argus_desktop_core::code::strip::trail;
use argus_desktop_core::code::stage::CodeGesture;
use argus_desktop_core::connection::page::{Call, Stage};
use argus_desktop_core::keys::keymap::Layer;
use argus_desktop_core::layout::dragged_width::{CODE_OUTLINE, CODE_TREE};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, relative, Bounds, Context, Entity, FocusHandle, Pixels, ScrollHandle, SharedString, Subscription, TestSupportExt as _, Window};

use super::file::{self, CodeFile};
use super::side::{lit_child, Lit};
use crate::chrome::header::slots::{Offer, Slots};
use crate::chrome::stage::route::narrow;
use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::keys::field::Registered;
use crate::keys::layer::Layers;
use crate::kit::area;
use crate::kit::field::Field;
use crate::kit::ghosts::ghosts;
use crate::kit::icon_button::icon_button;
use crate::kit::icons::Glyph;
use crate::kit::split::Split;
use crate::layout::holders::Holder;
use crate::theme::scale::{css, rem};
use crate::theme::Tokens;
use widths::{Frame, Tree};

pub struct CodeStage {
    pub(super) heard: Entity<Heard>,
    pub(super) caller: Caller,
    /// The one field over the tree and the glob field of the search.
    pub(super) find: Field,
    pub(super) glob: Field,
    /// What the session was last told each field says, so a field put back to what the page says
    /// sends nothing.
    pub(super) known_find: SharedString,
    pub(super) known_glob: SharedString,
    /// Where the keys rest while no field holds them: the stage itself.
    pub(super) focus: FocusHandle,
    /// Where each list was scrolled, kept while another is up and across the pages that follow.
    pub(super) tree_scroll: ScrollHandle,
    pub(super) jump_scroll: ScrollHandle,
    pub(super) hits_scroll: ScrollHandle,
    /// Whether the ignored fold is open: the window's memory, since a redraw does not forget it.
    pub(super) unfolded: bool,
    /// The file read, its header, ruler and comment box.
    file: Entity<CodeFile>,
    tree_split: Entity<Split>,
    outline_split: Entity<Split>,
    /// The area the stage was laid out in, whose width the columns' rules ask.
    area: Rc<Cell<Bounds<Pixels>>>,
    /// Whether the stage was up at the last update, to take the keys once as it appears.
    was_up: bool,
    /// Whether a list was open, to give the keys back when it closes.
    was_listing: bool,
    /// Whether the comment box was open, to give the keys back when it closes.
    was_commenting: bool,
    /// The row the cursor was on when the open list was last scrolled to it.
    lit: Option<Lit>,
    _following: Subscription,
    _typing: [Subscription; 2],
    _fields: [Registered; 2],
}

impl CodeStage {
    pub fn new(heard: Entity<Heard>, caller: Caller, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let tokens = *Tokens::global(cx);
        let find = Field::new(argus_desktop_core::code::side::FIND_HINT, window, cx);
        let glob = Field::new(argus_desktop_core::code::side::GLOB_HINT, window, cx);
        let split = |cx: &mut Context<Self>, id, label, usual: f32, bounds: argus_desktop_core::layout::dragged_width::Bounds, holder: Holder, mirrored: bool, report: fn(Option<u32>) -> Call| {
            let (holding, report_to) = (heard.clone(), caller.clone());
            cx.new(|cx| {
                let split = Split::new(id, label, usual, bounds.min as f32, bounds.max as f32, cx)
                    .reporting(move |width, _, _| report_to.call(report(width)))
                    .holding(move |on, cx| {
                        holding.update(cx, |heard, cx| {
                            let changed = if on { heard.hold_resize(holder) } else { heard.release_resize(holder) };
                            if changed {
                                cx.notify();
                            }
                        });
                    });
                if mirrored { split.mirrored() } else { split }
            })
        };
        let tree_split = split(cx, "code-tree-sash", "Resize the files", tokens.code_tree_w, CODE_TREE, Holder::CodeTree, false, |width| Call::CodeTreeWidth { width });
        let outline_split = split(cx, "code-outline-sash", "Resize the outline", tokens.code_outline_w, CODE_OUTLINE, Holder::CodeOutline, true, |width| Call::CodeOutlineWidth { width });
        let file = cx.new(|cx| CodeFile::new(heard.clone(), caller.clone(), window, cx));
        let (find_typing, find_keys) = super::side::wire_find(&find, &heard, &caller, window, cx);
        let (glob_typing, glob_keys) = super::side::wire_glob(&glob, &heard, &caller, window, cx);
        // The strip names the checkout and its branch, and carries the button for this view's keys.
        let (saying, helping) = (heard.clone(), caller.clone());
        let stage = Self {
            heard: heard.clone(),
            caller,
            find,
            glob,
            known_find: SharedString::default(),
            known_glob: SharedString::default(),
            focus: cx.focus_handle(),
            tree_scroll: ScrollHandle::new(),
            jump_scroll: ScrollHandle::new(),
            hits_scroll: ScrollHandle::new(),
            unfolded: false,
            file,
            tree_split,
            outline_split,
            area: Rc::default(),
            was_up: false,
            was_listing: false,
            was_commenting: false,
            lit: None,
            _following: cx.observe_in(&heard, window, |this, heard, window, cx| this.follow(&heard, window, cx)),
            _typing: [find_typing, glob_typing],
            _fields: [find_keys, glob_keys],
        };
        Slots::of(cx).offer(
            Stage::Code,
            Offer::default().says(move |cx| Says { trail: trail(saying.read(cx).code.as_deref()), ..Says::default() }).tools(move |_, cx| {
                let tokens = *Tokens::global(cx);
                let caller = helping.clone();
                icon_button("code-help", "Keys for this view", false, &tokens)
                    .tooltip(crate::kit::tip::tip("Keys for this view (?)"))
                    .child(Glyph::Help.at(tokens.icon_sm))
                    .on_click(move |_, _, _| caller.call(Call::Command { action: argus_desktop_core::keys::keymap::Action::Shortcuts }))
                    .into_any_element()
            }),
        );
        Layers::of(cx).register(Layer::Code, Rc::new(keys::StageKeys { stage: cx.weak_entity() }));
        stage
    }

    pub fn focus_handle(&self) -> &FocusHandle {
        &self.focus
    }

    /// Sends a gesture of the stage; a held window (the daemon gone) sends nothing.
    pub(crate) fn send(&self, gesture: CodeGesture, cx: &gpui_kit::App) {
        if !self.heard.read(cx).held() {
            self.caller.call(Call::Code { gesture });
        }
    }

    /// Follows the session: the fields say what the page does unless they are being typed in, the
    /// keys come to the stage as it appears and to the field a list opened for, and a list
    /// brings the row the cursor moved to into view.
    fn follow(&mut self, heard: &Entity<Heard>, window: &mut Window, cx: &mut Context<Self>) {
        let (page, rail) = {
            let heard = heard.read(cx);
            (heard.code.clone(), heard.rail.clone())
        };
        let up = rail.stage == Stage::Code;
        if up && !self.was_up {
            window.focus(&self.focus, cx);
        }
        self.was_up = up;
        let usual = |dragged: Option<u32>, usual: f32| dragged.map_or(usual, |width| width as f32);
        let tokens = *Tokens::global(cx);
        self.tree_split.update(cx, |split, cx| split.follow(usual(rail.code_tree_width, tokens.code_tree_w), cx));
        self.outline_split.update(cx, |split, cx| split.follow(usual(rail.code_outline_width, tokens.code_outline_w), cx));
        let Some(page) = page.filter(|page| page.open) else {
            cx.notify();
            return;
        };
        self.file.update(cx, |file, cx| file.follow(&page, window, cx));
        self.show_find(&page, window, cx);
        // The box takes the keys as it opens (in the file); as it closes they come back to the stage,
        // unless a list opened in the same breath has them.
        let commenting = page.comment.is_some();
        if self.was_commenting && !commenting && !self.find.typing(window, cx) {
            window.focus(&self.focus, cx);
        }
        self.was_commenting = commenting;
        if let Some(search) = &page.search {
            if !self.glob.typing(window, cx) && self.glob.text(cx) != search.glob {
                self.known_glob = SharedString::from(search.glob.clone());
                self.glob.put(&search.glob, window, cx);
            }
        }
        let listing = argus_desktop_core::code::side::body(&page) != argus_desktop_core::code::side::Body::Tree;
        if listing && !self.was_listing {
            self.find.focus(window, cx);
        } else if !listing && self.was_listing && self.find.typing(window, cx) {
            window.focus(&self.focus, cx);
        }
        self.was_listing = listing;
        let lit = lit_child(&page, self.unfolded);
        if lit != self.lit {
            self.lit = lit;
            match lit {
                Some(Lit::Tree(at)) => self.tree_scroll.scroll_to_item(at),
                Some(Lit::Jump(at)) => self.jump_scroll.scroll_to_item(at),
                Some(Lit::Hit(at)) => self.hits_scroll.scroll_to_item(at),
                None => {}
            }
        }
        cx.notify();
    }

    /// The width a column is drawn at: while the pointer holds its sash, the width it gives, under
    /// the narrow window's cap as the stylesheet puts it last; otherwise `ruled`, which already
    /// folds in the width the session last said.
    fn drawn(&self, split: &Entity<Split>, ruled: f32, narrow: Option<f32>, cx: &gpui_kit::App) -> f32 {
        let split = split.read(cx);
        if !split.dragging() {
            return ruled;
        }
        narrow.map_or(split.width(), |cap| split.width().min(cap))
    }

}


impl Render for CodeStage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        let root = div().id("code").test_support().key_context("Code").track_focus(&self.focus).size_full().text_color(tokens.fg_muted);
        let (page, rail, opened) = {
            let heard = self.heard.read(cx);
            (heard.code.clone().filter(|page| page.open), heard.rail.clone(), heard.opened.clone())
        };
        let Some(page) = page else {
            return root.child(div().id("code-loading").test_support().aria_label("Loading").size_full().child(ghosts("code-ghosts", &[0.3, 0.9, 0.85, 0.6], &tokens))).into_any_element();
        };
        let viewport = css(window.viewport_size().width, window.rem_size());
        let stage = match self.area.get().size.width {
            width if width > gpui_kit::px(0.0) => css(width, window.rem_size()),
            _ => viewport,
        };
        let frame = Frame {
            stage,
            narrow: narrow(viewport, tokens.narrow_below),
            dragged_tree: rail.code_tree_width,
            dragged_outline: rail.code_outline_width,
            searching: page.search.is_some(),
            alone: page.path.is_none(),
        };
        let side = self.side(&page, window, &tokens, cx);
        let mut files = div().id("code-files").test_support().relative().h_full().min_h_0().border_r_1().border_color(tokens.line).child(side).child(
            div().absolute().top_0().bottom_0().right_0().w(rem(tokens.sp_6)).child(self.tree_split.clone()),
        );
        files = match widths::tree(frame, &tokens) {
            Tree::Fixed { width, at_most } => {
                let width = self.drawn(&self.tree_split, width, frame.narrow.then_some(tokens.code_tree_narrow), cx);
                files.flex_shrink_0().w(rem(width)).max_w(relative(at_most))
            }
            Tree::Alone { at_most } => files.w(rem(at_most)),
        };
        // With no file read the header is only the way back, over the card or the start panel; with
        // one, the file draws it with everything else.
        let middle = if page.path.is_none() {
            let body = match selected_match(&page) {
                Some(found) => self.match_card(&found, &tokens, cx).into_any_element(),
                None => self.start(&start(&page, &opened), &tokens, cx).into_any_element(),
            };
            v_flex()
                .size_full()
                .min_h_0()
                .children(file::header(&page, &self.heard, &self.caller, &tokens))
                .child(div().flex_1().min_h_0().child(body))
                .into_any_element()
        } else {
            self.file.clone().into_any_element()
        };
        let outline = (page.outline && page.path.is_some()).then(|| widths::outline(frame, &tokens)).flatten();
        root.child(area::measure(self.area.clone()))
            .child(
                h_flex()
                    .size_full()
                    .child(files)
                    .child(v_flex().id("code-file").test_support().flex_1().min_w(rem(tokens.code_file_min_w)).h_full().min_h_0().overflow_hidden().child(middle))
                    .children(outline.map(|(width, at_most)| {
                        let width = self.drawn(&self.outline_split, width, frame.narrow.then_some(tokens.code_outline_narrow), cx);
                        self.outline(&page, width, at_most, &tokens, cx)
                    })),
            )
            .into_any_element()
    }
}

#[cfg(test)]
pub(crate) mod tests;
