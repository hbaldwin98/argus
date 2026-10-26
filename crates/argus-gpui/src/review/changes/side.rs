//! The Changes pane's list column, as the prototype draws it: a head saying what is listed (the
//! changes or the checkout's files and how many of each, with the switch between a flat list and
//! one by folder or the Changed filter, and the field that finds a file), the lists (`lists`, as
//! the lines of `lines`) or the files (`tree`), and the commit box under the lists (`commit`). A
//! review of one commit lists its files flat (`flat`), and a review from a daemon too old to stage
//! picks its side. The field that jumps among the files (`f`) stands in for the find field while open.
//!
//! Which files there are, which one is lit and what the field matches is the
//! session's (`ReviewPage`); a press sends its call and changes nothing. The
//! one thing held here is the field's own text: once it is open the text is
//! the field's, and the session's echo of a letter can land after the next is
//! typed, so a page never sets the text of an open field, only empties it when
//! it shuts (SHARED-4). The field's keys are the `Jump` place's, answered in the key walk
//! (`keys/field`): Up and Down (or Ctrl N and Ctrl P) step, Enter chooses and Escape shuts the
//! field and not the stage, run ahead of the input's own handling of them.

use std::collections::HashSet;

use argus_desktop_core::connection::page::{Call, ReviewClick, ReviewJumpGesture, ReviewPage};
use argus_desktop_core::keys::keymap::Place;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{h_flex, v_flex, ActiveTheme as _, Sizable as _, VirtualListScrollHandle};
use gpui_kit::Focusable as _;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, Context, Entity, Role, ScrollStrategy, SharedString, Subscription, Window};

use crate::kit::drag::{self, Drag};
use crate::kit::icons::Glyph;
use crate::kit::tip::tip;
use crate::review::changes::lists::ghost;
use crate::review::changes::commit::CommitBox;
use crate::review::changes::lists::{Areas, Carried};
use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::keys::field::{Fields, Registered};
use crate::review::keys;
use crate::kit::segment;
use crate::theme::scale::rem;
use crate::theme::Tokens;

pub struct Files {
    pub(super) heard: Entity<Heard>,
    caller: Caller,
    jump: Entity<InputState>,
    /// The find field, which narrows the lists to the files whose path holds its words: the
    /// window's own, never sent.
    find: Entity<InputState>,
    pub(super) query: String,
    pub(super) scroll: VirtualListScrollHandle,
    /// A file being carried between the lists, and what the carry takes the keys with.
    pub(super) drag: Drag<Carried>,
    pub(super) keys: gpui_kit::FocusHandle,
    /// Where each list was laid out, for a drop.
    pub(super) areas: Areas,
    /// The lists are by folder rather than flat: the window's own, never sent.
    pub(super) tree: bool,
    /// The Files tab is shown rather than the changes, its folders open, and whether it shows only
    /// what holds a change: the window's own.
    pub(super) checkout: bool,
    pub(super) folders: HashSet<String>,
    pub(super) changed_only: bool,
    commit: Entity<CommitBox>,
    /// What the field held when the session was last told, so the empty it is
    /// put back to when it shuts says nothing.
    known: SharedString,
    open: bool,
    /// The file lit when the list last scrolled to it.
    lit: Option<usize>,
    _subscriptions: [Subscription; 2],
    /// The field's place in the key walk: the `Jump` place's keys are run before its own.
    _field: Registered,
}

/// The files shown, as indices into `files`: the jump field's matches while
/// it is open, else every one in daemon order.
pub fn order(page: &ReviewPage) -> Vec<usize> {
    match &page.jump {
        Some(jump) => jump.matches.clone(),
        None => (0..page.files.len()).collect(),
    }
}

/// The file lit: what Enter would open while the field is, else the one the cursor is in.
pub fn lit(page: &ReviewPage) -> Option<usize> {
    match &page.jump {
        Some(jump) => jump.at,
        None => page.file,
    }
}

/// A folder as the row says it: without its trailing slash, or the phrase for the root.
pub fn folder(dir: &str) -> &str {
    match dir.trim_end_matches('/') {
        "" => "repository root",
        dir => dir,
    }
}

/// The list the column holds: the checkout's files, both sides' changes, or one flat list (a
/// commit's files, or the jump field's matches).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shows {
    Tree,
    Sections,
    Flat,
}

/// A file was pressed in a list, to be read: the pane turns to its diff when it is narrow.
pub struct Opened;

impl gpui_kit::EventEmitter<Opened> for Files {}

impl Files {
    pub fn new(heard: Entity<Heard>, caller: Caller, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let jump = cx.new(|cx| InputState::new(window, cx).placeholder("Jump to a file"));
        let typing = cx.subscribe(&jump, |this, input, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                let text = input.read(cx).value();
                if text != this.known {
                    this.known = text.clone();
                    this.ask(Call::ReviewJump { gesture: ReviewJumpGesture::Query { text: text.to_string() } }, cx);
                }
            }
        });
        let asking = cx.weak_entity();
        let field = Fields::of(cx).register_place(window, jump.focus_handle(cx), Place::Jump, move |action, _, cx| {
            let Some(gesture) = keys::jump_gesture(action) else { return false };
            asking.update(cx, |files, cx| files.ask(Call::ReviewJump { gesture }, cx)).is_ok()
        });
        let commit = cx.new(|cx| CommitBox::new(heard.clone(), caller.clone(), window, cx));
        let find = cx.new(|cx| InputState::new(window, cx).placeholder("Find a changed file"));
        let finding = cx.subscribe(&find, |this, input, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.query = input.read(cx).value().trim().to_lowercase();
                cx.notify();
            }
        });
        Self {
            heard,
            caller,
            jump,
            find,
            query: String::new(),
            scroll: VirtualListScrollHandle::new(),
            drag: Drag::new(),
            keys: cx.focus_handle(),
            areas: Areas::default(),
            tree: false,
            checkout: false,
            folders: HashSet::new(),
            changed_only: false,
            commit,
            known: SharedString::default(),
            open: false,
            lit: None,
            _subscriptions: [typing, finding],
            _field: field,
        }
    }

    /// Follows the session: an open field keeps the keys, a shut one lets its
    /// text go, and the list brings the lit file into view when it moved.
    pub fn follow(&mut self, page: &ReviewPage, window: &mut Window, cx: &mut Context<Self>) {
        let open = page.jump.is_some();
        if open {
            self.jump.update(cx, |jump, cx| jump.focus(window, cx));
        } else if self.open {
            self.known = SharedString::default();
            self.jump.update(cx, |jump, cx| jump.set_value("", window, cx));
        }
        self.open = open;
        self.commit.update(cx, |commit, cx| commit.follow(page.staging.as_ref(), window, cx));
        let now = lit(page);
        if now != self.lit {
            self.lit = now;
            if let Some(at) = now.and_then(|_| self.lit_row(page)) {
                self.scroll.scroll_to_item(at, ScrollStrategy::Nearest);
            }
        }
        cx.notify();
    }

    /// Which list is shown, of the three the column holds.
    fn shows(&self, page: &ReviewPage) -> Shows {
        if self.checkout && page.jump.is_none() && page.tree.is_some() {
            Shows::Tree
        } else if !page.sections.is_empty() && page.jump.is_none() {
            Shows::Sections
        } else {
            Shows::Flat
        }
    }

    /// The row of the shown list the lit file is drawn in.
    fn lit_row(&self, page: &ReviewPage) -> Option<usize> {
        match self.shows(page) {
            Shows::Tree => self.tree_lit_row(page),
            Shows::Sections => self.sections_lit_row(page),
            Shows::Flat => lit(page).and_then(|file| order(page).iter().position(|shown| *shown == file)),
        }
    }

    pub(super) fn ask(&self, call: Call, cx: &Context<Self>) {
        if !self.heard.read(cx).held() {
            self.caller.call(call);
        }
    }

    /// `.side-head`: what the list holds and how, and the field that finds in it.
    fn side(&self, page: &ReviewPage, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let head = v_flex()
            .id("review-files-head")
            .flex_shrink_0()
            .p(rem(10.0))
            .gap(rem(tokens.sp_8))
            .border_b_1()
            .border_color(tokens.line);
        if page.base == "commit" {
            let words = SharedString::from(page.commit.clone().unwrap_or_default());
            return head
                .child(
                    div()
                        .id("review-commit")
                        .test_support()
                        .aria_label(words.clone())
                        .min_w_0()
                        .truncate()
                        .font_family(cx.theme().mono_font_family.clone())
                        .text_xs()
                        .text_color(tokens.fg_muted)
                        .child(words),
                )
                .into_any_element();
        }
        let row = h_flex().gap(rem(tokens.sp_6)).items_center();
        if page.sections.is_empty() {
            // A daemon too old to stage reads one side at a time, picked here.
            let count = |n: Option<usize>| n.map_or_else(|| "\u{2026}".to_owned(), |n| n.to_string());
            let pill = |id: &'static str, label: &'static str, n: Option<usize>, staged: bool, cx: &mut Context<Self>| {
                segment::pill(id, Role::RadioButton, (page.base == "staged") == staged, tokens)
                    .child(label)
                    .child(segment::count(count(n), tokens))
                    .on_click(cx.listener(move |this, _, _, cx| this.ask(Call::ReviewClick { target: ReviewClick::Side { staged } }, cx)))
            };
            return head
                .child(row.child(
                    segment::track("review-side", "Side", Role::RadioGroup, tokens)
                        .child(pill("review-side-unstaged", "Unstaged", page.unstaged, false, cx))
                        .child(pill("review-side-staged", "Staged", page.staged, true, cx)),
                ))
                .into_any_element();
        }
        let files: usize = {
            let mut paths: Vec<&str> = page.sections.iter().flat_map(|s| s.files.iter().map(|f| f.file.path.as_str())).collect();
            paths.sort_unstable();
            paths.dedup();
            paths.len()
        };
        let mode = |id: &'static str, words: &'static str, glyph: Glyph, tree: bool, on: bool, cx: &mut Context<Self>| {
            ghost(id, words, tokens)
                .aria_selected(on)
                .when(on, |button| button.bg(tokens.fill).text_color(tokens.fg).border_color(tokens.line_strong))
                .tooltip(tip(words))
                .child(glyph.at(14.0))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.tree = tree;
                    cx.notify();
                }))
        };
        let checkout = self.checkout;
        let all = page.tree.as_ref().filter(|tree| tree.listed).map_or_else(|| "\u{2026}".to_owned(), |tree| tree.files.len().to_string());
        let tab = |id: &'static str, words: &'static str, n: String, files: bool, cx: &mut Context<Self>| {
            segment::pill(id, Role::Tab, checkout == files, tokens).child(words).child(segment::count(n, tokens)).on_click(cx.listener(move |this, _, window, cx| this.show_tab(files, window, cx)))
        };
        let changed = self.changed_only;
        let find = if checkout { "Find a file in the checkout" } else { "Find a changed file" };
        head.child(
            row.child(
                segment::track("review-list-what", "What the list shows", Role::TabList, tokens)
                    .child(tab("review-list-changes", "Changes", files.to_string(), false, cx))
                    .when(page.tree.is_some(), |tabs| tabs.child(tab("review-list-files", "Files", all, true, cx))),
            )
            .child(div().flex_1())
            .when(!checkout, |row| {
                row.child(mode("review-list-flat", "As a list", Glyph::List, false, !self.tree, cx)).child(mode("review-list-tree", "By folder", Glyph::Tree, true, self.tree, cx))
            })
            .when(checkout, |row| {
                row.child(
                    ghost("review-list-changed", "Changed", tokens)
                        .aria_selected(changed)
                        .when(changed, |button| button.bg(tokens.fill).text_color(tokens.fg).border_color(tokens.line_strong))
                        .tooltip(tip("Only folders and files with changes"))
                        .child("Changed")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.changed_only = !this.changed_only;
                            cx.notify();
                        })),
                )
            }),
        )
        .child(
            div()
                .id("review-find-field")
                .min_w_0()
                .child(Input::new(&self.find).id("review-find").small().h(rem(tokens.control_h_sm)).prefix(Glyph::Search.at(14.0)).aria_label(find)),
        )
        .into_any_element()
    }

    /// A tab pressed: the Files tab asks for the checkout's files again each time it is shown.
    fn show_tab(&mut self, files: bool, window: &mut Window, cx: &mut Context<Self>) {
        if files {
            self.ask(Call::ReviewFiles, cx);
        }
        if self.checkout != files {
            self.checkout = files;
            let words = if files { "Find a file in the checkout" } else { "Find a changed file" };
            self.find.update(cx, |find, cx| find.set_placeholder(words, window, cx));
        }
        cx.notify();
    }

    /// The field that jumps among the files, over them while it is open. The page's `Changed
    /// files` label is not drawn over the list: the Changes tab and the side's head already say
    /// what the list is.
    fn jump(&self, tokens: &Tokens) -> impl IntoElement {
        div()
            .id("review-jump-field")
            .flex_shrink_0()
            .min_w_0()
            .child(Input::new(&self.jump).id("review-jump").small().h(rem(tokens.control_h)).aria_label("Jump to a file"))
    }
}

impl Render for Files {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        let Some(page) = self.heard.read(cx).review.clone() else {
            return div().into_any_element();
        };
        // Both sides' lists while the field is shut; its matches, of the side on screen, while open.
        let shows = self.shows(&page);
        let checkout = shows == Shows::Tree;
        let list = match shows {
            Shows::Tree => self.checkout(&page, &tokens, window, cx).into_any_element(),
            Shows::Sections => self.sections(&page, &tokens, window, cx).into_any_element(),
            Shows::Flat => self.list(&page, &tokens, window, cx).into_any_element(),
        };
        // Under the head, `.review-files`: 12px in from every edge, 8px between its parts.
        // `.side`: on the chrome colour, the head, the lists, and the commit box at the foot.
        v_flex()
            .id("review-files")
            .size_full()
            .min_w_0()
            .bg(tokens.bg_chrome)
            .child(self.side(&page, &tokens, cx))
            .when(page.jump.is_some(), |side| side.child(div().p(rem(10.0)).border_b_1().border_color(tokens.line).child(self.jump(&tokens))))
            .child(list)
            .when(page.staging.is_some() && !checkout, |side| side.child(self.commit.clone()))
            // The carry takes the keys here, so Escape drops it before the stage hears it.
            .child(
                div()
                    .id("review-files-keys")
                    .size_0()
                    .track_focus(&self.keys)
                    .on_key_down(cx.listener(|this, event: &gpui_kit::KeyDownEvent, window, cx| drag::escape(this, event, window, cx))),
            )
            .child(drag::watch(cx.weak_entity()))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_desktop_core::connection::page::{ReviewFile, ReviewJump};

    fn file(path: &str) -> ReviewFile {
        let (dir, name) = path.rsplit_once('/').map_or(("", path), |(dir, name)| (dir, name));
        ReviewFile {
            path: path.into(),
            dir: if dir.is_empty() { String::new() } else { format!("{dir}/") },
            name: name.into(),
            added: 1,
            removed: 0,
            kind: "modified",
        }
    }

    fn page(jump: Option<ReviewJump>) -> ReviewPage {
        ReviewPage {
            open: true,
            files: vec![file("a.rs"), file("src/b.rs"), file("src/c.rs")],
            file: Some(1),
            jump,
            ..ReviewPage::default()
        }
    }

    #[test]
    fn the_list_is_every_file_in_daemon_order_and_the_lit_one_is_the_cursors() {
        let page = page(None);
        assert_eq!(order(&page), [0, 1, 2]);
        assert_eq!(lit(&page), Some(1));
    }

    #[test]
    fn an_open_field_lists_only_its_matches_best_first_and_lights_what_enter_opens() {
        let page = page(Some(ReviewJump { query: "c".into(), matches: vec![2, 0], at: Some(2) }));
        assert_eq!(order(&page), [2, 0]);
        assert_eq!(lit(&page), Some(2), "the field's match, not the cursor's file");
        let none = ReviewJump { query: "zz".into(), matches: Vec::new(), at: None };
        assert_eq!(lit(&super::tests::page(Some(none))), None);
    }

    #[test]
    fn a_folder_is_said_without_its_slash_and_the_root_is_a_phrase() {
        assert_eq!(folder("crates/argus-hook/src/"), "crates/argus-hook/src");
        assert_eq!(folder(""), "repository root");
        assert_eq!(folder("/"), "repository root");
    }
}
