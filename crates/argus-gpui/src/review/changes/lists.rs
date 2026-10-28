//! The Changes pane's lists — staged, changed, untracked — and what a press on a file in them asks:
//! a click opens it on the side it is listed on, a double-click or its button moves it to the
//! other list, and a drag drops it there.
//!
//! Which files are in which list is the session's (`ReviewSection`): a move is a `Stage` call, and
//! the file changes lists when the diff read after the index moved says so. The drag is the
//! window's one helper (`kit::drag`), and a drop is read against where each list was last laid out.

use std::cell::RefCell;
use std::rc::Rc;

use argus_desktop_core::connection::page::{Call, ReviewPage};
use argus_desktop_core::review::staging::{ReviewSection, SectionFile, StageGesture};
use gpui_kit::component::{h_flex, v_flex, ActiveTheme as _};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{canvas, div, prelude::*, Bounds, Context, MouseButton, MouseDownEvent, Pixels, Point, SharedString, Window};

use crate::kit::button::{button, Look};
use crate::kit::icon_button::Node;
use crate::kit::counts::counts;
use crate::kit::icons::Glyph;
use crate::kit::drag::{Dragger, Step};
use crate::kit::tip::tip;
use crate::kit::long_list::long_list;
use crate::kit::reveal;
use crate::review::changes::lines::{change_id, height, lines, shown, Line, EMPTY_BOX, HEAD};
use crate::review::changes::flat::FILE_ROW;
use crate::review::changes::side::{folder, Files};
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// A file being carried from its list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Carried {
    pub path: String,
    /// The list it is in.
    pub staged: bool,
}

/// Where each list was last laid out, and whether it is the staged one, for a drop.
pub type Areas = Rc<RefCell<Vec<(bool, Bounds<Pixels>)>>>;

/// The list a point is over, as whether it is the staged one.
pub fn list_at(areas: &[(bool, Bounds<Pixels>)], at: Point<Pixels>) -> Option<bool> {
    areas.iter().find(|(_, bounds)| bounds.contains(&at)).map(|(staged, _)| *staged)
}

/// What a file carried to `onto` asks: a move across, or nothing when it is let go over its own list.
pub fn dropped(carried: &Carried, onto: Option<bool>) -> Option<StageGesture> {
    let onto = onto?;
    (onto != carried.staged).then(|| StageGesture::Files { staged: onto, paths: vec![carried.path.clone()] })
}

fn title(side: &str) -> &'static str {
    match side {
        "staged" => "Staged",
        "untracked" => "Untracked",
        _ => "Changes",
    }
}

/// The letter `git status` says of a file, the colour it is drawn in, and the word for it.
pub fn status(kind: &str, tokens: &Tokens) -> (&'static str, gpui_kit::Hsla, &'static str) {
    match kind {
        "added" => ("A", tokens.ok, "Added"),
        "deleted" => ("D", tokens.danger, "Deleted"),
        "renamed" => ("R", tokens.ref_, "Renamed"),
        "untracked" => ("U", tokens.info, "Untracked"),
        _ => ("M", tokens.changed, "Modified"),
    }
}

/// `.st`: the status letter on a square of its own colour.
pub fn badge(id: impl Into<gpui_kit::ElementId>, letter: &'static str, colour: gpui_kit::Hsla, said: &'static str, tokens: &Tokens, cx: &gpui_kit::App) -> impl IntoElement {
    div()
        .id(id)
        .flex_shrink_0()
        .size(rem(18.0))
        .rounded(rem(tokens.r_xs))
        .flex()
        .items_center()
        .justify_center()
        .bg(colour.opacity(0.14))
        .text_color(colour)
        .font_family(cx.theme().mono_font_family.clone())
        .text_size(rem(10.5))
        .weighted(tokens.weight_semibold)
        .child(letter)
        .tooltip(tip(said))
}

/// `.ghost`: a quiet button, muted until the pointer is on it.
pub fn ghost(id: impl Into<gpui_kit::ElementId>, words: impl Into<SharedString>, tokens: &Tokens) -> Node {
    button(id, words, Look::Quiet, true, tokens)
}

/// `.dirhead`: a folder's name over its files, when the list is by folder.
fn dirhead(dir: &str, tokens: &Tokens, cx: &gpui_kit::App) -> impl IntoElement {
    h_flex()
        .flex_shrink_0()
        .h(rem(tokens.sp_24))
        .pl(rem(14.0))
        .pr(rem(tokens.sp_12))
        .gap(rem(tokens.sp_6))
        .items_center()
        .text_color(tokens.fg_faint)
        .font_family(cx.theme().mono_font_family.clone())
        .text_size(rem(11.0))
        .child(Glyph::Folder.at(14.0))
        .child(div().min_w_0().truncate().child(SharedString::from(dir.to_owned())))
}

/// The lists with only the files whose path holds `query`, already lowercased; every file while it
/// is empty.
pub fn found(sections: &[ReviewSection], query: &str) -> Vec<ReviewSection> {
    sections
        .iter()
        .map(|section| ReviewSection {
            side: section.side,
            files: section.files.iter().filter(|f| query.is_empty() || f.file.path.to_lowercase().contains(query)).cloned().collect(),
        })
        .collect()
}

/// How many drafts wait on each file, by its path.
pub fn drafted(page: &ReviewPage) -> Vec<(String, usize)> {
    let mut counts: Vec<(String, usize)> = Vec::new();
    for draft in page.drafts.iter().flat_map(|d| d.drafts.iter()) {
        match counts.iter_mut().find(|(path, _)| *path == draft.path) {
            Some((_, n)) => *n += 1,
            None => counts.push((draft.path.clone(), 1)),
        }
    }
    counts
}

impl Files {
    /// The lists, drawn only where they are in view: a large checkout's changes run to thousands.
    pub(super) fn sections(&self, page: &ReviewPage, tokens: &Tokens, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.areas.borrow_mut().clear();
        let found = found(&page.sections, &self.query);
        let finding = !self.query.is_empty();
        let shown = shown(&found, finding);
        let list = v_flex().id("review-sections").test_support().flex_1().min_h_0();
        if finding && shown.is_empty() {
            return list.child(
                div()
                    .id("review-find-none")
                    .test_support()
                    .px(rem(tokens.sp_16))
                    .py(rem(tokens.sp_24))
                    .typed(tokens.type_small)
                    .text_color(tokens.fg_faint)
                    .text_center()
                    .child("No changed file matches."),
            );
        }
        let lines = lines(&found, &shown, self.tree);
        let heights: Vec<f32> = lines.iter().map(|line| height(line, tokens)).collect();
        let talked = drafted(page);
        list.child(
            long_list(cx.entity(), "review-section-lines", &heights, window.rem_size(), &self.scroll, move |this, at, _, cx| {
                let tokens = *Tokens::global(cx);
                let line = &lines[at];
                let section = &found[line.list()];
                let last = lines.get(at + 1).is_none_or(|next| matches!(next, Line::Head(_)));
                let drawn = match line {
                    Line::Head(k) => this.head(*k, section, &tokens, cx).into_any_element(),
                    Line::Empty(_) => this.empty(section, &tokens).into_any_element(),
                    Line::Folder(_, dir) => dirhead(dir, &tokens, cx).into_any_element(),
                    Line::File { list, at, folders } => {
                        this.change(change_id(*list, *at), &section.files[*at], section.side == "staged", *folders, &talked, &tokens, cx).into_any_element()
                    }
                };
                this.framed(at, line, section, last, drawn, &tokens).into_any_element()
            })
            .size_full()
            .pt(rem(tokens.sp_4))
            .pb(rem(10.0)),
        )
    }

    /// The line of the lists the file on screen is drawn in.
    pub(super) fn sections_lit_row(&self, page: &ReviewPage) -> Option<usize> {
        let found = found(&page.sections, &self.query);
        let shown = shown(&found, !self.query.is_empty());
        lines(&found, &shown, self.tree).iter().position(|line| matches!(line, Line::File { list, at, .. } if found[*list].files[*at].on))
    }

    /// Whether a file carried from the other list is over `staged`'s, which lights it as a target.
    fn targeted(&self, staged: bool) -> bool {
        self.drag_over().filter(|over| *over == staged && self.carried().is_some_and(|c| c.staged != staged)).is_some()
    }

    /// `.group`: a line in its list's frame, lit as a drop target while a file from the other list
    /// is carried over it. The frame is drawn a line at a time, opened by the head and closed by
    /// the list's last line, and each line says where it was laid out, for a drop.
    #[allow(clippy::too_many_arguments)]
    fn framed(&self, at: usize, line: &Line, section: &ReviewSection, last: bool, drawn: gpui_kit::AnyElement, tokens: &Tokens) -> impl IntoElement {
        let staged = section.side == "staged";
        let target = self.targeted(staged);
        let areas = self.areas.clone();
        let first = matches!(line, Line::Head(_));
        let corner = rem(tokens.r_md);
        let edge = if target { tokens.accent.opacity(0.55) } else { gpui_kit::transparent_black() };
        // The head carries the list's id, which the rest of its lines cannot share.
        let frame = if first { div().id(("review-section", line.list())) } else { div().id(("review-section-line", at)) };
        frame
            .test_support()
            .when(first, |frame| frame.aria_label(SharedString::from(title(section.side))))
            .relative()
            .size_full()
            .mx(rem(tokens.sp_4))
            .border_l_1()
            .border_r_1()
            .when(first, |frame| frame.border_t_1().rounded_t(corner))
            .when(last, |frame| frame.border_b_1().rounded_b(corner))
            .border_color(edge)
            .when(target, |frame| frame.bg(tokens.accent.opacity(0.09)))
            .child(canvas(move |bounds, _, _| areas.borrow_mut().push((staged, bounds)), |_, _, _, _| {}).absolute().size_full())
            .child(drawn)
    }

    /// `.section`: the list's type in capitals, its count fainter, the move of all at its end.
    fn head(&self, k: usize, section: &ReviewSection, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let staged = section.side == "staged";
        let target = self.targeted(staged);
        let all = if staged { "Unstage all" } else { "Stage all" };
        let paths: Vec<String> = section.files.iter().map(|f| f.file.path.clone()).collect();
        h_flex()
            .gap(rem(tokens.sp_8))
            .items_center()
            .h(rem(HEAD))
            .pl(rem(tokens.sp_12))
            .pr(rem(6.0))
            .typed(tokens.type_label)
            .text_color(if target { tokens.accent } else { tokens.fg_faint })
            .child(SharedString::from(title(section.side).to_uppercase()))
            .child(div().text_color(tokens.fg_fainter).child(SharedString::from(section.files.len().to_string())))
            .child(div().flex_1())
            .when(!paths.is_empty(), |head| {
                head.child(
                    ghost(("review-section-all", k), all, tokens)
                        .child(all)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.ask(Call::Stage { gesture: StageGesture::Files { staged: !staged, paths: paths.clone() } }, cx);
                        })),
                )
            })
    }

    /// The box an empty list shows, saying how a file comes to it.
    fn empty(&self, section: &ReviewSection, tokens: &Tokens) -> impl IntoElement {
        let target = self.targeted(section.side == "staged");
        div().pt(rem(tokens.sp_2)).pb(rem(tokens.sp_6)).px(rem(tokens.sp_6)).child(
            div()
                .h(rem(EMPTY_BOX))
                .flex()
                .items_center()
                .justify_center()
                .rounded(rem(tokens.r_sm))
                .border_1()
                .border_dashed()
                .border_color(if target { tokens.accent } else { tokens.line_strong })
                .typed(tokens.type_small)
                .text_color(if target { tokens.accent } else { tokens.fg_fainter })
                .text_center()
                .child("Drag a file here, or double-click it, to stage it"),
        )
    }

    /// `.row`: one file of a list — its status, name, folder and counts — and, hovered or lit, the
    /// move across where the counts were. `folders` is whether the row says its folder, which a
    /// tree's folder head says for it.
    #[allow(clippy::too_many_arguments)]
    fn change(&self, id: usize, file: &SectionFile, staged: bool, folders: bool, talked: &[(String, usize)], tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let (letter, colour, said) = status(file.file.kind, tokens);
        let path = file.file.path.clone();
        let carried = Carried { path: path.clone(), staged };
        let moving = self.carried().is_some_and(|c| *c == carried) && self.drag.is_dragging();
        let group = SharedString::from(format!("review-change-{id}"));
        let across = if staged { "Unstage" } else { "Stage" };
        let move_path = path.clone();
        let waiting = talked.iter().find(|(p, _)| *p == path).map(|(_, n)| *n);
        let hint = if staged { "Double-click or drag to Changes to unstage" } else { "Double-click or drag to Staged to stage" };
        h_flex()
            .id(("review-change", id))
            .test_support()
            .group(group.clone())
            .aria_selected(file.on)
            .aria_label(SharedString::from(format!("{} {}", title(if staged { "staged" } else { file.file.kind }), path)))
            .flex_shrink_0()
            .h(rem(FILE_ROW))
            .mx(rem(tokens.sp_2))
            .pl(rem(tokens.sp_8))
            .pr(rem(tokens.sp_6))
            .gap(rem(tokens.sp_8))
            .items_center()
            .rounded(rem(tokens.r_sm))
            .border_1()
            .border_color(if file.on { tokens.line_strong } else { gpui_kit::transparent_black() })
            .typed(tokens.type_body)
            .text_color(if file.on { tokens.fg } else { tokens.fg_muted })
            .cursor_pointer()
            .when(moving, |row| row.opacity(0.45))
            .when(file.on, |row| row.bg(tokens.bg_selected))
            .when(!file.on, |row| row.hover(|style| style.bg(tokens.fill.opacity(0.6)).text_color(tokens.fg)))
            .tooltip(tip(SharedString::from(format!("{path}\n{hint}"))))
            // A second press of a double-click moves the file; a first is held, to be a click or a drag.
            .on_mouse_down(MouseButton::Left, cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                if event.click_count >= 2 {
                    this.drag.cancel();
                    this.ask(Call::Stage { gesture: StageGesture::File { staged, path: path.clone() } }, cx);
                } else {
                    this.drag.press(event.position, carried.clone());
                }
            }))
            .child(badge(("review-change-status", id), letter, colour, said, tokens, cx))
            // The name gives way only once the folder beside it has gone, and is then cut short too.
            .child(
                div()
                    .flex_shrink(1.0)
                    .min_w(rem(48.0))
                    .truncate()
                    .text_color(tokens.fg)
                    .weighted(tokens.weight_medium)
                    .child(SharedString::from(file.file.name.clone())),
            )
            .when(file.partly, |row| {
                row.child(
                    div()
                        .id(("review-change-partly", id))
                        .test_support()
                        .aria_label("Partly staged")
                        .flex_shrink_0()
                        .size(rem(6.0))
                        .rounded_full()
                        .bg(tokens.yellow)
                        .tooltip(tip("Partly staged")),
                )
            })
            .when_some(waiting, |row, n| {
                let words = SharedString::from(format!("{n} comment{} waiting in your review", if n == 1 { "" } else { "s" }));
                row.child(
                    h_flex()
                        .id(("review-change-talk", id))
                        .test_support()
                        .aria_label(words.clone())
                        .flex_shrink_0()
                        .gap(rem(3.0))
                        .items_center()
                        .text_color(tokens.accent)
                        .font_family(SharedString::from(tokens.font_mono))
                        .text_size(rem(11.0))
                        .tooltip(tip(words))
                        .child(Glyph::Talk.at(12.0))
                        .child(SharedString::from(n.to_string())),
                )
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .typed(tokens.type_small)
                    .text_color(tokens.fg_faint)
                    .when(folders, |dir| dir.child(SharedString::from(folder(&file.file.dir).to_owned()))),
            )
            // `.nums` and `.stage` share the row's end: the counts until the row is hovered or lit,
            // then the move across.
            .child(
                h_flex()
                    .flex_shrink_0()
                    .items_center()
                    .when(!file.on, |end| {
                        end.child(reveal::off_hover(group.clone(), div().text_size(rem(11.0)).child(counts(file.file.added, file.file.removed, tokens))))
                    })
                    .child(reveal::on_hover(
                        group,
                        file.on,
                        ghost(("review-change-move", id), SharedString::from(format!("{across} {move_path}")), tokens)
                            .tooltip(tip(SharedString::from(format!("{across} file"))))
                            .child(if staged { Glyph::Minus.at(14.0) } else { Glyph::Plus.at(14.0) })
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.ask(Call::Stage { gesture: StageGesture::File { staged, path: move_path.clone() } }, cx);
                            })),
                    )),
            )
    }

    fn carried(&self) -> Option<&Carried> {
        self.drag.payload()
    }

    /// The list the carried file is over, while it is being dragged.
    fn drag_over(&self) -> Option<bool> {
        let at = self.drag.at().filter(|_| self.drag.is_dragging())?;
        list_at(&self.areas.borrow(), at)
    }
}

impl Dragger for Files {
    type Payload = Carried;

    fn drag(&self) -> &crate::kit::drag::Drag<Carried> {
        &self.drag
    }

    fn drag_mut(&mut self) -> &mut crate::kit::drag::Drag<Carried> {
        &mut self.drag
    }

    fn keys(&self) -> &gpui_kit::FocusHandle {
        &self.keys
    }

    fn dragged(&mut self, step: Step<Carried>, _window: &mut Window, cx: &mut Context<Self>) {
        match step {
            Step::Clicked(carried) => {
                cx.emit(super::side::Opened);
                self.ask(Call::OpenChange { staged: carried.staged, path: carried.path }, cx)
            }
            Step::Dropped(carried, at) => {
                let onto = list_at(&self.areas.borrow(), at);
                if let Some(gesture) = dropped(&carried, onto) {
                    self.ask(Call::Stage { gesture }, cx);
                }
            }
            Step::Began(_) | Step::Moved(_) | Step::Cancelled(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::{point, px, size};

    fn carried(staged: bool) -> Carried {
        Carried { path: "a.rs".into(), staged }
    }

    #[test]
    fn a_file_dropped_on_the_other_list_moves_and_on_its_own_stays() {
        let to_staged = dropped(&carried(false), Some(true));
        assert!(matches!(to_staged, Some(StageGesture::Files { staged: true, ref paths }) if paths == &["a.rs"]));
        assert!(dropped(&carried(false), Some(false)).is_none());
        assert!(dropped(&carried(true), None).is_none(), "let go over neither list");
        assert!(matches!(dropped(&carried(true), Some(false)), Some(StageGesture::Files { staged: false, .. })));
    }

    #[test]
    fn the_find_field_keeps_the_files_whose_path_holds_its_words_in_every_list() {
        use argus_desktop_core::connection::page::ReviewFile;
        let file = |path: &str| SectionFile {
            file: ReviewFile { path: path.into(), dir: String::new(), name: path.into(), added: 1, removed: 0, kind: "modified" },
            partly: false,
            on: false,
        };
        let sections = vec![
            ReviewSection { side: "staged", files: vec![file("src/Docks.rs")] },
            ReviewSection { side: "unstaged", files: vec![file("src/lib.rs"), file("docs/controls.md")] },
        ];
        let paths = |found: Vec<ReviewSection>| found.into_iter().map(|s| s.files.into_iter().map(|f| f.file.path).collect::<Vec<_>>()).collect::<Vec<_>>();
        assert_eq!(paths(found(&sections, "doc")), [vec!["src/Docks.rs"], vec!["docs/controls.md"]], "any case, any list");
        assert_eq!(paths(found(&sections, "")), [vec!["src/Docks.rs"], vec!["src/lib.rs", "docs/controls.md"]]);
    }

    #[test]
    fn a_point_is_over_the_list_laid_out_under_it() {
        let areas = vec![
            (true, Bounds { origin: point(px(0.), px(0.)), size: size(px(100.), px(50.)) }),
            (false, Bounds { origin: point(px(0.), px(60.)), size: size(px(100.), px(50.)) }),
        ];
        assert_eq!(list_at(&areas, point(px(10.), px(10.))), Some(true));
        assert_eq!(list_at(&areas, point(px(10.), px(70.))), Some(false));
        assert_eq!(list_at(&areas, point(px(10.), px(55.))), None, "between the two");
    }
}
