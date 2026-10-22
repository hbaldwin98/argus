//! The changed-files column of the review: which side is read, the files in
//! daemon order (or the matches of the jump field, best first) and the field
//! that jumps among them.
//!
//! Which files there are, which one is lit and what the field matches is the
//! session's (`ReviewPage`); a press sends its call and changes nothing. The
//! one thing held here is the field's own text: once it is open the text is
//! the field's, and the session's echo of a letter can land after the next is
//! typed, so a page never sets the text of an open field, only empties it when
//! it shuts (SHARED-4). The field's keys are its own: Up and Down (or Ctrl N
//! and Ctrl P) step, Enter chooses and Escape shuts the field and not the
//! stage, caught ahead of the input's own handling of them.

use argus_desktop_core::connection::page::{Call, ReviewClick, ReviewJumpGesture, ReviewPage};
use argus_desktop_core::keys::keymap::Action;
use gpui_kit::component::input::{Enter, Input, InputEvent, InputState, MoveDown, MoveUp};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme as _, Sizable as _};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, px, Context, Entity, KeyDownEvent, Role, ScrollHandle, SharedString, Subscription, Window};

use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::keys::page_key;
use crate::review::keys;
use crate::review::segment;
use crate::style::Typed as _;
use crate::theme::scale::Type;
use crate::theme::Tokens;

pub struct Files {
    heard: Entity<Heard>,
    caller: Caller,
    jump: Entity<InputState>,
    scroll: ScrollHandle,
    /// What the field held when the session was last told, so the empty it is
    /// put back to when it shuts says nothing.
    known: SharedString,
    open: bool,
    /// The file lit when the list last scrolled to it.
    lit: Option<usize>,
    _subscriptions: [Subscription; 1],
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
        Self { heard, caller, jump, scroll: ScrollHandle::new(), known: SharedString::default(), open: false, lit: None, _subscriptions: [typing] }
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
        let now = lit(page);
        if now != self.lit {
            self.lit = now;
            if let Some(at) = now.and_then(|file| order(page).iter().position(|shown| *shown == file)) {
                self.scroll.scroll_to_item(at);
            }
        }
        cx.notify();
    }

    fn ask(&self, call: Call, cx: &Context<Self>) {
        if !self.heard.read(cx).held() {
            self.caller.call(call);
        }
    }

    fn gesture(&self, action: Action, cx: &Context<Self>) {
        if let Some(gesture) = keys::jump_gesture(action) {
            self.ask(Call::ReviewJump { gesture }, cx);
        }
    }

    /// The side the review reads, or the commit it is of.
    fn side(&self, page: &ReviewPage, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let row = h_flex().flex_shrink_0().items_center().gap(px(tokens.sp_8));
        if page.base == "commit" {
            return row
                .child(
                    div()
                        .id("review-commit")
                        .test_support()
                        .min_w_0()
                        .truncate()
                        .font_family(cx.theme().mono_font_family.clone())
                        .text_xs()
                        .text_color(tokens.fg_muted)
                        .child(SharedString::from(page.commit.clone().unwrap_or_default())),
                )
                .into_any_element();
        }
        let count = |n: Option<usize>| n.map_or_else(|| "\u{2026}".to_owned(), |n| n.to_string());
        let pill = |id: &'static str, label: &'static str, n: Option<usize>, staged: bool, cx: &mut Context<Self>| {
            segment::pill(id, Role::RadioButton, (page.base == "staged") == staged, tokens)
                .child(label)
                .child(segment::count(count(n), tokens))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.ask(Call::ReviewClick { target: ReviewClick::Side { staged } }, cx)
                }))
        };
        row.child(
            segment::track("review-side", "Side", Role::RadioGroup, tokens)
                .child(pill("review-side-unstaged", "Unstaged", page.unstaged, false, cx))
                .child(pill("review-side-staged", "Staged", page.staged, true, cx)),
        )
        .into_any_element()
    }

    /// The label above the files, or the field that jumps among them.
    fn head(&self, page: &ReviewPage, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let head = h_flex().flex_shrink_0().items_center();
        if page.jump.is_none() {
            // `.mlabel`: 600 at 12px, muted, 6px over and 2px under.
            return head
                .px(px(tokens.sp_8))
                .pt(px(tokens.sp_6))
                .pb(px(tokens.sp_2))
                .typed(Type { weight: tokens.weight_semibold, size: tokens.fs_sm, leading: 1.4 })
                .text_color(tokens.fg_muted)
                .child("Changed files")
                .into_any_element();
        }
        head.child(
            div()
                .id("review-jump-field")
                .flex_1()
                .min_w_0()
                // Ahead of the input's own handling of each, which would move
                // its caret, or leave the field to the stage under it.
                .capture_action(cx.listener(|this, _: &MoveDown, _, cx| {
                    cx.stop_propagation();
                    this.gesture(Action::Next, cx);
                }))
                .capture_action(cx.listener(|this, _: &MoveUp, _, cx| {
                    cx.stop_propagation();
                    this.gesture(Action::Previous, cx);
                }))
                .capture_action(cx.listener(|this, _: &Enter, _, cx| {
                    cx.stop_propagation();
                    this.gesture(Action::Choose, cx);
                }))
                // Ctrl N and Ctrl P step as in fzf, and must not reach the pane.
                .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                    let Some(page) = page_key::from_keystroke(&event.keystroke) else { return };
                    if let Some(gesture) = keys::jump(&page) {
                        cx.stop_propagation();
                        this.ask(Call::ReviewJump { gesture }, cx);
                    }
                }))
                .child(Input::new(&self.jump).id("review-jump").small().h(px(tokens.control_h)).aria_label("Jump to a file")),
        )
        .into_any_element()
    }

    fn list(&self, page: &ReviewPage, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let lit = lit(page);
        let rows: Vec<_> = order(page).into_iter().filter_map(|i| page.files.get(i).map(|file| (i, file))).collect();
        let (fill, selected_hover) = (tokens.fill, tokens.selected_hover);
        v_flex()
            .id("review-files-list")
            .test_support()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .gap(px(tokens.sp_2))
            .children(rows.into_iter().map(|(i, file)| {
                let on = Some(i) == lit;
                let tooltip = SharedString::from(file.path.clone());
                // `.rfile`: 4px over 8px of padding, less the edge that the lit row
                // draws as its mark, so every row is one size.
                h_flex()
                    .id(("review-file", i))
                    .test_support()
                    .aria_selected(on)
                    .aria_label(tooltip.clone())
                    .flex_shrink_0()
                    .items_start()
                    .gap(px(tokens.sp_6))
                    .py(px(tokens.sp_4 - 1.0))
                    .px(px(tokens.sp_8 - 1.0))
                    .rounded(px(tokens.r_sm))
                    .border_1()
                    .border_color(if on { tokens.line_strong } else { gpui_kit::transparent_black() })
                    .text_size(px(tokens.fs_sm))
                    .line_height(px(tokens.fs_sm * 1.45))
                    .text_color(tokens.fg)
                    .cursor_pointer()
                    .when(on, |row| row.bg(tokens.bg_selected).hover(move |style| style.bg(selected_hover)))
                    .when(!on, |row| row.hover(move |style| style.bg(fill)))
                    .tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.ask(Call::ReviewClick { target: ReviewClick::File { file: i } }, cx)
                    }))
                    // The name is what is scanned for, so it leads; its folder is the
                    // line under it and is what is cut short.
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap(px(tokens.sp_2))
                            .child(
                                div()
                                    .truncate()
                                    .typed(Type { weight: tokens.weight_semibold, size: tokens.fs_md, leading: 1.45 })
                                    .child(SharedString::from(file.name.clone())),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(tokens.fs_xs))
                                    .line_height(px(tokens.fs_xs * 1.45))
                                    .text_color(tokens.fg_faint)
                                    .when(!file.dir.trim_end_matches('/').is_empty(), |dir| {
                                        dir.font_family(cx.theme().mono_font_family.clone())
                                    })
                                    .child(SharedString::from(folder(&file.dir).to_owned())),
                            ),
                    )
                    .child(div().text_color(tokens.ok).child(SharedString::from(format!("+{}", file.added))))
                    .child(div().text_color(tokens.danger).child(SharedString::from(format!("\u{2212}{}", file.removed))))
            }))
            .when_some(page.jump.as_ref().filter(|jump| jump.matches.is_empty()), |list, jump| {
                list.child(
                    div()
                        .id("review-jump-none")
                        .test_support()
                        .typed(tokens.type_small)
                        .text_color(tokens.fg_muted)
                        .child(SharedString::from(format!("No file matches \u{201c}{}\u{201d}.", jump.query))),
                )
            })
    }
}

impl Render for Files {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        let Some(page) = self.heard.read(cx).review.clone() else {
            return div().into_any_element();
        };
        // `.review-files`: 12px in from every edge, 8px between its parts.
        v_flex()
            .id("review-files")
            .size_full()
            .min_w_0()
            .p(px(tokens.sp_12))
            .gap(px(tokens.sp_8))
            .child(self.head(&page, &tokens, cx))
            .child(self.side(&page, &tokens, cx))
            .child(self.list(&page, &tokens, cx))
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
