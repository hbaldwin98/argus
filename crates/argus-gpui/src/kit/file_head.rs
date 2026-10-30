//! The head over a file, one component for Code, Git Changes and Git History
//! (`project/Kit.dc.html`, "File head"): [status letter] name, folder, `+a −r`, one meta, a
//! spacer, the tools.
//!
//! It is always one 44px line. The folder is the only part that shrinks, from its left (`path`);
//! the name, the counts, the meta and the tools keep their width, and nothing wraps.

use gpui_kit::component::h_flex;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, AnyElement, ElementId, Hsla, SharedString};

use super::icon_button::Node;
use crate::style::Typed as _;

use super::counts::counts;
use super::path::left_cut;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// What a file head says; the tools are passed beside it.
pub struct Head<'a> {
    /// The change's letter and its tone (Git); none for a file that is only being read (Code).
    pub status: Option<(&'a str, Hsla)>,
    pub name: &'a str,
    pub dir: &'a str,
    /// `+a −r`, left out when both are none.
    pub counts: Option<(usize, usize)>,
    /// `Ln 20 · Rust`, `2 hunks`, `in <sha>`.
    pub meta: Option<SharedString>,
}

/// The head. Its accessible label is the whole path; the folder's box is `("file-head-dir", 0)`.
pub fn file_head(id: impl Into<ElementId>, head: &Head, tools: Vec<AnyElement>, tokens: &Tokens) -> Node {
    let full = if head.dir.is_empty() { head.name.to_owned() } else { format!("{}/{}", head.dir, head.name) };
    let mono = SharedString::from(tokens.font_mono);
    h_flex()
        .id(id)
        .test_support()
        .aria_label(SharedString::from(full))
        .flex_shrink_0()
        .w_full()
        .min_w_0()
        .h(rem(tokens.sp_44))
        .pl(rem(tokens.sp_16))
        .pr(rem(tokens.sp_12))
        .gap(rem(10.0))
        .items_center()
        .overflow_hidden()
        .border_b_1()
        .border_color(tokens.line)
        .whitespace_nowrap()
        .children(head.status.map(|(letter, tone)| {
            div()
                .flex_none()
                .w(rem(tokens.icon_sm))
                .text_center()
                .font_family(mono.clone())
                .text_size(rem(tokens.fs_xs))
                .weighted(tokens.weight_semibold)
                .text_color(tone)
                .child(SharedString::from(letter.to_owned()))
        }))
        .child(div().flex_none().text_size(rem(tokens.fs_md)).weighted(tokens.weight_semibold).text_color(tokens.fg).child(SharedString::from(head.name.to_owned())))
        .when(!head.dir.is_empty(), |line| line.child(left_cut("file-head-dir", SharedString::from(head.dir.to_owned()), tokens)))
        .children(head.counts.filter(|(a, r)| a + r > 0).map(|(a, r)| counts(a, r, tokens).text_size(rem(tokens.fs_sm))))
        .children(head.meta.clone().map(|meta| div().id("file-head-meta").test_support().aria_label(meta.clone()).flex_none().text_size(rem(tokens.fs_sm)).text_color(tokens.fg_muted).child(meta)))
        .child(div().flex_1().min_w_0())
        .child(h_flex().flex_none().gap(rem(tokens.sp_4)).items_center().children(tools))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kit::host;
    use crate::kit::tools;
    use gpui_kit::TestAppContext;

    const LONG: &str = "crates/argus-desktop-core/src/dictation/hold/with/a/very/deep/tail";

    fn tools_for(tokens: &Tokens) -> Vec<AnyElement> {
        vec![
            tools::ghost("t-whole", "Whole file", None, false, tokens).into_any_element(),
            tools::ghost("t-edit", "Open in editor", Some("E"), false, tokens).into_any_element(),
            tools::primary("t-stage", "Stage file", tokens).into_any_element(),
        ]
    }

    fn open(cx: &mut TestAppContext, width: f32, dir: &'static str) -> gpui_kit::AnyWindowHandle {
        host::open(cx, width, move |tokens| {
            let head = Head { status: Some(("M", tokens.changed)), name: "mod.rs", dir, counts: Some((12, 4)), meta: Some("2 hunks".into()) };
            file_head("fh", &head, tools_for(tokens), tokens).into_any_element()
        })
    }

    /// Kit.dc.html: 44px, one line, at every width.
    #[gpui_kit::test]
    fn the_head_is_one_44px_line_at_every_width(cx: &mut TestAppContext) {
        for width in [1100.0, 700.0, 520.0] {
            let handle = open(cx, width, LONG);
            let (_, _, w, h) = host::bounds(cx, handle, "fh");
            assert_eq!(h, 44.0, "{width}");
            assert!(w <= width, "{width}");
        }
    }

    /// Kit.dc.html: the folder is the only thing that shrinks; the tools keep their buttons whole.
    #[gpui_kit::test]
    fn the_folder_gives_way_and_the_tools_stay_whole(cx: &mut TestAppContext) {
        let wide = open(cx, 1100.0, LONG);
        let narrow = open(cx, 640.0, LONG);
        let (_, _, wide_dir, _) = host::bounds(cx, wide, ("file-head-dir", 0usize));
        let (_, _, narrow_dir, _) = host::bounds(cx, narrow, ("file-head-dir", 0usize));
        assert!(narrow_dir < wide_dir, "the folder shrank: {narrow_dir} < {wide_dir}");
        let (_, _, stage, _) = host::bounds(cx, wide, "t-stage");
        let (_, _, stage_narrow, _) = host::bounds(cx, narrow, "t-stage");
        assert_eq!(stage, stage_narrow, "the primary button keeps its width");
        let (_, _, edit, _) = host::bounds(cx, wide, "t-edit");
        let (_, _, edit_narrow, _) = host::bounds(cx, narrow, "t-edit");
        assert_eq!(edit, edit_narrow, "so does the ghost");
    }
}
