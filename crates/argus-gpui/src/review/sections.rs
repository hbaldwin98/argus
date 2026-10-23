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

use crate::kit::counts::counts;
use crate::kit::drag::{Dragger, Step};
use crate::kit::tip::tip;
use crate::review::files::{folder, Files};
use crate::style::Typed as _;
use crate::theme::scale::{rem, Type};
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

/// The letter `git status` says of a file, and the colour it is drawn in.
fn status(file: &SectionFile, tokens: &Tokens) -> (&'static str, gpui_kit::Hsla) {
    match file.file.kind {
        "added" => ("A", tokens.ok),
        "deleted" => ("D", tokens.danger),
        "renamed" => ("R", tokens.ref_),
        "untracked" => ("U", tokens.info),
        _ => ("M", tokens.changed),
    }
}

impl Files {
    /// The three lists, the untracked one only when it holds a file.
    pub(super) fn sections(&self, page: &ReviewPage, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        self.areas.borrow_mut().clear();
        let shown: Vec<(usize, &ReviewSection)> = page.sections.iter().enumerate().filter(|(_, s)| s.side != "untracked" || !s.files.is_empty()).collect();
        v_flex()
            .id("review-sections")
            .test_support()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .gap(rem(tokens.sp_8))
            .children(shown.into_iter().map(|(k, section)| self.section(k, section, tokens, cx)))
    }

    fn section(&self, k: usize, section: &ReviewSection, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let staged = section.side == "staged";
        let areas = self.areas.clone();
        let target = self.drag_over().filter(|over| *over == staged && self.carried().is_some_and(|c| c.staged != staged)).is_some();
        let paths: Vec<String> = section.files.iter().map(|f| f.file.path.clone()).collect();
        let all = if staged { "Unstage all" } else { "Stage all" };
        let head = h_flex()
            .gap(rem(tokens.sp_8))
            .items_center()
            .h(rem(28.0))
            .px(rem(tokens.sp_8))
            .text_size(rem(tokens.fs_xs))
            .text_color(if target { tokens.accent } else { tokens.fg_faint })
            .weighted(tokens.weight_semibold)
            .child(SharedString::from(title(section.side).to_uppercase()))
            .child(div().text_color(tokens.fg_fainter).child(SharedString::from(section.files.len().to_string())))
            .child(div().flex_1())
            .when(!paths.is_empty(), |head| {
                head.child(
                    div()
                        .id(("review-section-all", k))
                        .test_support()
                        .aria_label(SharedString::from(all))
                        .px(rem(tokens.sp_6))
                        .rounded(rem(tokens.r_sm))
                        .text_color(tokens.fg_muted)
                        .cursor_pointer()
                        .hover(|button| button.bg(tokens.fill).text_color(tokens.fg))
                        .child(all)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.ask(Call::Stage { gesture: StageGesture::Files { staged: !staged, paths: paths.clone() } }, cx);
                        })),
                )
            });
        v_flex()
            .id(("review-section", k))
            .test_support()
            .aria_label(SharedString::from(title(section.side)))
            .relative()
            .flex_shrink_0()
            .rounded(rem(tokens.r_md))
            .when(target, |list| list.bg(tokens.drop_target).border_1().border_color(tokens.accent))
            .when(!target, |list| list.border_1().border_color(gpui_kit::transparent_black()))
            .child(canvas(move |bounds, _, _| areas.borrow_mut().push((staged, bounds)), |_, _, _, _| {}).absolute().size_full())
            .child(head)
            .when(section.files.is_empty(), |list| {
                let hint = if staged { "Nothing staged. Drop files here, or tick lines in the diff." } else { "Nothing else changed." };
                list.child(
                    div()
                        .mx(rem(tokens.sp_6))
                        .mb(rem(tokens.sp_6))
                        .p(rem(tokens.sp_8))
                        .rounded(rem(tokens.r_sm))
                        .border_1()
                        .border_dashed()
                        .border_color(if target { tokens.accent } else { tokens.line_strong })
                        .text_size(rem(tokens.fs_xs))
                        .text_color(if target { tokens.accent } else { tokens.fg_fainter })
                        .text_center()
                        .child(hint),
                )
            })
            .children(section.files.iter().enumerate().map(|(i, file)| self.change(k * 10_000 + i, file, staged, tokens, cx)))
    }

    /// One file of a list: its status, name and folder, the counts, and on hover the move across.
    fn change(&self, id: usize, file: &SectionFile, staged: bool, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let (letter, colour) = status(file, tokens);
        let path = file.file.path.clone();
        let carried = Carried { path: path.clone(), staged };
        let moving = self.carried().is_some_and(|c| *c == carried) && self.drag.is_dragging();
        let (fill, selected_hover) = (tokens.fill, tokens.selected_hover);
        let group = SharedString::from(format!("review-change-{id}"));
        let across = if staged { "Unstage" } else { "Stage" };
        let move_path = path.clone();
        h_flex()
            .id(("review-change", id))
            .test_support()
            .group(group.clone())
            .aria_selected(file.on)
            .aria_label(SharedString::from(format!("{} {}", title(if staged { "staged" } else { file.file.kind }), path)))
            .flex_shrink_0()
            .h(rem(28.0))
            .mx(rem(tokens.sp_2))
            .px(rem(tokens.sp_6))
            .gap(rem(tokens.sp_8))
            .items_center()
            .rounded(rem(tokens.r_sm))
            .border_1()
            .border_color(if file.on { tokens.line_strong } else { gpui_kit::transparent_black() })
            .text_size(rem(tokens.fs_sm))
            .text_color(tokens.fg_muted)
            .when(moving, |row| row.opacity(0.45))
            .when(file.on, |row| row.bg(tokens.bg_selected).text_color(tokens.fg).hover(move |style| style.bg(selected_hover)))
            .when(!file.on, |row| row.hover(move |style| style.bg(fill)))
            .tooltip(tip(SharedString::from(path.clone())))
            // A second press of a double-click moves the file; a first is held, to be a click or a drag.
            .on_mouse_down(MouseButton::Left, cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                if event.click_count >= 2 {
                    this.drag.cancel();
                    this.ask(Call::Stage { gesture: StageGesture::File { staged, path: path.clone() } }, cx);
                } else {
                    this.drag.press(event.position, carried.clone());
                }
            }))
            .child(
                div()
                    .flex_shrink_0()
                    .size(rem(18.0))
                    .rounded(rem(4.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(colour.opacity(0.14))
                    .text_color(colour)
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_size(rem(tokens.fs_xs))
                    .weighted(tokens.weight_semibold)
                    .child(letter),
            )
            .child(div().flex_shrink_0().text_color(tokens.fg).typed(Type { weight: tokens.weight_semibold, size: tokens.fs_sm, leading: 1.4 }).child(SharedString::from(file.file.name.clone())))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(rem(tokens.fs_xs))
                    .text_color(tokens.fg_faint)
                    .child(SharedString::from(folder(&file.file.dir).to_owned())),
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
            // The lit row and a hovered one offer the move across where the counts were.
            .child(
                div()
                    .flex_shrink_0()
                    .when(file.on, |counts| counts.invisible())
                    .group_hover(group.clone(), |counts| counts.invisible())
                    .child(counts(file.file.added, file.file.removed, tokens)),
            )
            .child(
                div()
                    .id(("review-change-move", id))
                    .test_support()
                    .aria_label(SharedString::from(format!("{across} {move_path}")))
                    .absolute()
                    .right(rem(tokens.sp_8))
                    .when(!file.on, |button| button.invisible().group_hover(group, |button| button.visible()))
                    .px(rem(tokens.sp_6))
                    .rounded(rem(tokens.r_sm))
                    .border_1()
                    .border_color(tokens.line_strong)
                    .bg(tokens.bg_surface)
                    .text_size(rem(tokens.fs_xs))
                    .text_color(tokens.fg)
                    .cursor_pointer()
                    .child(across)
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.ask(Call::Stage { gesture: StageGesture::File { staged, path: move_path.clone() } }, cx);
                    })),
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
            Step::Clicked(carried) => self.ask(Call::OpenChange { staged: carried.staged, path: carried.path }, cx),
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
