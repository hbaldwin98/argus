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
use argus_desktop_core::review::staging::{ReviewSection, StageGesture};
use gpui_kit::component::v_flex;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{canvas, div, prelude::*, Bounds, Context, Pixels, Point, SharedString, Window};

use crate::kit::button::{button, Look};
use crate::kit::icon_button::Node;
use crate::kit::drag::{Dragger, Step};
use crate::kit::long_list::long_list;
use crate::kit::section;
use crate::review::changes::lines::{change_id, height, lines, shown, Line, EMPTY_BOX};
use crate::review::changes::side::Files;
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

pub(super) fn title(side: &str) -> &'static str {
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

/// `.ghost`: a quiet button, muted until the pointer is on it.
pub fn ghost(id: impl Into<gpui_kit::ElementId>, words: impl Into<SharedString>, tokens: &Tokens) -> Node {
    button(id, words, Look::Quiet, true, tokens)
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
        let lines = lines(&found, &shown, self.tree, &self.closed);
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
                    Line::Folder { .. } => this.folder_row(at, line, &tokens, cx),
                    Line::File { list, at: file, depth } => this.change(change_id(*list, *file), at, &section.files[*file], section.side == "staged", *depth, &talked, &tokens, cx),
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
        lines(&found, &shown, self.tree, &self.closed).iter().position(|line| matches!(line, Line::File { list, at, .. } if found[*list].files[*at].on))
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

    /// The list's section head (`kit::section`): its type in capitals, its count, the move of all
    /// at its end.
    fn head(&self, k: usize, section: &ReviewSection, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let staged = section.side == "staged";
        let all = if staged { "Unstage all" } else { "Stage all" };
        let paths: Vec<String> = section.files.iter().map(|f| f.file.path.clone()).collect();
        let words = match section.side {
            "staged" => "Staged",
            "untracked" => "Untracked",
            _ => "Changed",
        };
        let action = (!paths.is_empty()).then(|| {
            section::action(("review-section-all", k), all, tokens)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.ask(Call::Stage { gesture: StageGesture::Files { staged: !staged, paths: paths.clone() } }, cx);
                }))
                .into_any_element()
        });
        let head = section::head(("review-section-head", k), words, Some(section.files.len()), action, tokens);
        if self.targeted(staged) { head.text_color(tokens.accent) } else { head }
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
                .child("Drop a file here, or double-click it"),
        )
    }

    pub(super) fn carried(&self) -> Option<&Carried> {
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
    use argus_desktop_core::review::staging::SectionFile;
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
