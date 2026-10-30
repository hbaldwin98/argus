//! A path as the lists say it: the name leads, the folder follows faint, and when the line is too
//! short it is the folder that gives way, cut from its left so the end of the path survives
//! (`project/Kit.dc.html`'s `.dir`, which the page cuts with `direction: rtl`).
//!
//! GPUI has no text direction, so the folder is a box that clips and lays its text against the
//! right edge: whatever does not fit is lost on the left.

use gpui_kit::component::h_flex;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{canvas, div, font, point, prelude::*, px, ContentMask, ElementId, SharedString, TextAlign, TextRun};

use super::icon_button::Node;
use crate::style::Typed as _;

use crate::theme::scale::{pixels, rem};
use crate::theme::Tokens;

/// The least of a folder that is kept before it is dropped altogether.
pub const LEAST: f32 = 40.0;

/// `text` in the faint mono face (its box is `(id, 0)`, the text `(id, 1)`), cut from the left when
/// its box is narrower than it, with a leading `…` where it was cut. The box has the text's own
/// width until the line asks it to shrink, and shrinks no further than [`LEAST`].
///
/// The inner text only gives the box its width, so it is drawn clear; the canvas over it paints the
/// visible text, because only at paint time is it known whether the text fit and where to cut it.
pub fn left_cut(id: &'static str, text: impl Into<SharedString>, tokens: &Tokens) -> Node {
    let text: SharedString = text.into();
    let (family, size, colour) = (SharedString::from(tokens.font_mono), tokens.fs_sm, tokens.fg_faint);
    let painted = text.clone();
    div()
        .id((id, 0usize))
        .test_support()
        .relative()
        .flex_shrink(1.0)
        .min_w(rem(LEAST))
        .overflow_hidden()
        .flex()
        .justify_end()
        .whitespace_nowrap()
        .font_family(family.clone())
        .text_size(rem(size))
        .text_color(gpui_kit::transparent_black())
        .child(div().id((id, 1usize)).test_support().flex_none().child(text))
        .child(
            canvas(
                |_, _, _| (),
                move |bounds, (), window, cx| {
                    let face = font(family.clone());
                    let size = pixels(size, window.rem_size());
                    let shape = |window: &mut gpui_kit::Window, line: String| {
                        let run = TextRun { len: line.len(), font: face.clone(), color: colour, background_color: None, underline: None, strikethrough: None };
                        window.text_system().shape_line(SharedString::from(line), size, &[run], None)
                    };
                    let room = bounds.size.width;
                    let whole = shape(window, painted.to_string());
                    let line = if whole.width() <= room {
                        whole
                    } else {
                        // The least start whose tail, after the ellipsis, still fits the box.
                        let starts: Vec<usize> = painted.char_indices().map(|(at, _)| at).collect();
                        let (mut low, mut high) = (0, starts.len());
                        while low < high {
                            let mid = (low + high) / 2;
                            if shape(window, format!("\u{2026}{}", &painted[starts[mid]..])).width() <= room {
                                high = mid;
                            } else {
                                low = mid + 1;
                            }
                        }
                        let start = starts.get(low).copied().unwrap_or(painted.len());
                        shape(window, format!("\u{2026}{}", &painted[start..]))
                    };
                    let origin = point(bounds.origin.x + (room - line.width()).max(px(0.0)), bounds.origin.y);
                    window.with_content_mask(Some(ContentMask { bounds }), |window| {
                        line.paint(origin, bounds.size.height, TextAlign::Left, None, window, cx).ok();
                    });
                },
            )
            .absolute()
            .size_full(),
        )
}

/// A name then its folder on one line: the name whole and semibold, the folder after it faint and
/// cut from the left (its box is `("path-dir", 0)`). Nothing wraps. `name` and `dir` are core's
/// `path_parts`.
pub fn path_label(id: impl Into<ElementId>, name: &str, dir: &str, tokens: &Tokens) -> Node {
    let full = if dir.is_empty() { name.to_owned() } else { format!("{dir}/{name}") };
    h_flex()
        .id(id)
        .test_support()
        .aria_label(SharedString::from(full))
        .min_w_0()
        .gap(rem(tokens.sp_8))
        .items_baseline()
        .whitespace_nowrap()
        .child(div().flex_none().weighted(tokens.weight_semibold).text_color(tokens.fg).child(SharedString::from(name.to_owned())))
        .when(!dir.is_empty(), |row| row.child(left_cut("path-dir", SharedString::from(dir.to_owned()), tokens)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kit::host;
    use gpui_kit::TestAppContext;

    const LONG: &str = "crates/argus-desktop-core/src/dictation/hold/with/a/very/deep/tail";

    /// Kit.dc.html file head: the folder is the only thing that shrinks, and it loses its left.
    #[gpui_kit::test]
    fn a_folder_too_long_for_its_box_is_cut_from_the_left(cx: &mut TestAppContext) {
        let handle = host::open(cx, 600.0, |tokens| div().w(rem(120.0)).child(left_cut("dir", LONG, tokens)).into_any_element());
        let (box_left, _, box_width, _) = host::bounds(cx, handle, ("dir", 0usize));
        let (text_left, _, text_width, _) = host::bounds(cx, handle, ("dir", 1usize));
        assert!(text_width > box_width, "the folder is longer than its box");
        assert!(text_left < box_left, "the start is lost");
        assert!((text_left + text_width - (box_left + box_width)).abs() < 1.0, "the end of the path sits at the box's right edge");
    }

    #[gpui_kit::test]
    fn a_label_stays_inside_its_line_with_the_name_whole(cx: &mut TestAppContext) {
        let handle = host::open(cx, 600.0, |tokens| div().w(rem(260.0)).child(path_label("p", "hold.rs", LONG, tokens)).into_any_element());
        let (_, _, width, _) = host::bounds(cx, handle, "p");
        assert!(width <= 260.5, "the label fits its line: {width}");
        let (_, _, dir, _) = host::bounds(cx, handle, ("path-dir", 0usize));
        assert!(dir >= LEAST - 0.5, "the folder keeps its least: {dir}");
    }

    #[gpui_kit::test]
    fn a_short_folder_keeps_its_own_width(cx: &mut TestAppContext) {
        let handle = host::open(cx, 600.0, |tokens| div().w(rem(600.0)).child(path_label("p", "mod.rs", "src/kit", tokens)).into_any_element());
        let (_, _, width, _) = host::bounds(cx, handle, ("path-dir", 0usize));
        assert!(width < 100.0, "the folder is as wide as its words, not the line: {width}");
    }
}
