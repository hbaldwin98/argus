//! The sheet's sections and the column that names them: six sections, each with its icon and, under
//! its name, the line about what it holds, and the one lit that is in view.
//!
//! A press on a name scrolls the body to its section; scrolling lights the last section whose top
//! has reached the top of the body (`spy` of the page's `preferences.ts`), so a long page says
//! where it is. A body scrolled to its foot lights its last section, whose top the short sections
//! under the tall Appearance never let reach the top, so a press on it lights what it pressed.

use std::rc::Rc;

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, App, Role, ScrollHandle, SharedString, TestSupportExt as _, Window};

use crate::kit::icons::Glyph;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

pub struct Section {
    pub name: &'static str,
    pub about: &'static str,
    pub icon: Glyph,
}

/// The sections in the order the sheet lists them.
pub const SECTIONS: [Section; 6] = [
    Section { name: "Appearance", about: "The theme, the background and the fonts", icon: Glyph::Workspace },
    Section { name: "Editor", about: "Where a file opens, and with what", icon: Glyph::Code },
    Section { name: "Notifications", about: "When Argus tells you something needs you", icon: Glyph::Bell },
    Section { name: "Features", about: "How the feature index opens", icon: Glyph::List },
    Section { name: "Views", about: "Where the links in a saved view open", icon: Glyph::Split },
    Section { name: "Connection", about: "The daemon this window reaches", icon: Glyph::Server },
];

/// The section in view: the last whose top is within `reach` of the body's top, or the last of all
/// when the body is at its foot; the first when the body has not been laid out.
pub fn current(tops: &[Option<f32>], body_top: f32, reach: f32, at_foot: bool) -> usize {
    if at_foot && tops.last().is_some_and(Option::is_some) {
        return tops.len() - 1;
    }
    let mut at = 0;
    for (index, top) in tops.iter().enumerate() {
        if top.is_some_and(|top| top - body_top <= reach) {
            at = index;
        }
    }
    at
}

/// The section in view of `body`, which holds the sections as its children in order. A child's bounds
/// are where layout put it before the body scrolled, so the offset is added to find where it is drawn.
pub fn in_view(body: &ScrollHandle, tokens: &Tokens) -> usize {
    let scrolled = f32::from(body.offset().y);
    let tops: Vec<Option<f32>> = (0..SECTIONS.len()).map(|at| body.bounds_for_item(at).map(|bounds| f32::from(bounds.origin.y) + scrolled)).collect();
    let room = f32::from(body.max_offset().y);
    current(&tops, f32::from(body.bounds().origin.y), tokens.sp_24, room > 0.0 && -scrolled >= room - 1.0)
}

/// What a name in the nav does when pressed.
pub type Press = Rc<dyn Fn(&mut Window, &mut App)>;

/// `.settings-nav`: a row per section, its name over the line about it, the one in view lit, and the
/// version at the foot. `presses` is what each name does when pressed, in the order of [`SECTIONS`].
pub fn column(lit: usize, version: &str, tokens: &Tokens, presses: Vec<Press>) -> impl IntoElement {
    v_flex()
        .id("settings-nav")
        .role(Role::Navigation)
        .aria_label("Settings sections")
        .flex_shrink_0()
        // The page's `width: 200px` is the content box: its padding and its border come on top.
        .w(rem(tokens.settings_nav_w + 2.0 * tokens.sp_12 + tokens.hairline))
        .h_full()
        .gap(rem(tokens.sp_2))
        .p(rem(tokens.sp_12))
        .border_r_1()
        .border_color(tokens.line)
        .children(SECTIONS.iter().enumerate().map(|(at, section)| {
            let go = presses[at].clone();
            h_flex()
                .id(("settings-nav-item", at))
                .role(Role::Button)
                .test_support()
                .aria_label(SharedString::from(section.name))
                .aria_selected(at == lit)
                .flex_shrink_0()
                .min_h(rem(tokens.control_h))
                .px(rem(tokens.sp_8))
                .py(rem(tokens.sp_6))
                .gap(rem(tokens.sp_8))
                .items_start()
                .rounded(rem(tokens.r_sm))
                .cursor_pointer()
                .text_color(tokens.fg)
                .when(at == lit, |row| row.bg(tokens.bg_selected))
                .when(at != lit, |row| row.hover(move |style| style.bg(tokens.fill)))
                .on_click(move |_, window, cx| go(window, cx))
                // The icon sits on the name's line, not centred on the two lines.
                .child(div().flex_shrink_0().pt(rem(tokens.sp_2)).text_color(tokens.fg_faint).child(section.icon.at(tokens.icon_sm)))
                .child(
                    v_flex()
                        .min_w_0()
                        .gap(rem(tokens.sp_2))
                        .child(SharedString::from(section.name))
                        .child(
                            div()
                                .id(("settings-nav-about", at))
                                .test_support()
                                .aria_label(SharedString::from(section.about))
                                .typed(tokens.type_small)
                                .text_color(tokens.fg_faint)
                                .child(SharedString::from(section.about)),
                        ),
                )
        }))
        .child(div().flex_1())
        .when(!version.is_empty(), |nav| nav.child(div().p(rem(tokens.sp_8)).typed(tokens.type_small).text_color(tokens.fg_muted).child(SharedString::from(format!("argus {version}")))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_section_in_view_is_the_last_whose_top_has_reached_the_body() {
        let tops = [Some(100.0), Some(500.0), Some(900.0)];
        assert_eq!(current(&tops, 100.0, 24.0, false), 0);
        assert_eq!(current(&[Some(-450.0), Some(-50.0), Some(350.0)], 100.0, 24.0, false), 1);
        assert_eq!(current(&[Some(-900.0), Some(-500.0), Some(120.0)], 100.0, 24.0, false), 2, "within reach counts");
        assert_eq!(current(&[Some(-900.0), Some(-500.0), Some(125.0)], 100.0, 24.0, false), 1, "beyond it does not");
    }

    #[test]
    fn a_body_at_its_foot_has_its_last_section_in_view() {
        assert_eq!(current(&[Some(-900.0), Some(-500.0), Some(300.0)], 100.0, 24.0, true), 2);
        assert_eq!(current(&[None, None, None], 0.0, 24.0, true), 0, "not laid out is still the first");
    }

    #[test]
    fn a_body_not_yet_laid_out_has_the_first_section_in_view() {
        assert_eq!(current(&[None, None, None], 0.0, 24.0, false), 0);
        assert_eq!(current(&[], 0.0, 24.0, false), 0);
        assert_eq!(current(&[], 0.0, 24.0, true), 0);
    }
}
