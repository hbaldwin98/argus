//! How a [`Tokens`] becomes gpui-kit's theme, the one place the kit's roles are filled.
//!
//! gpui-kit's `Theme` has no role for `changed` or the `fg_fainter` grey, so
//! views read `Tokens` for what is ours and `cx.theme()` for what a kit
//! component also paints; both come from the same tokens, here.

use gpui_kit::component::highlighter::HighlightTheme;
use gpui_kit::component::{Colorize as _, Theme, ThemeColor, ThemeMode};
use gpui_kit::{px, App, Hsla};

use super::Tokens;

/// gpui-kit's roles for these tokens, starting from its dark or light set so
/// any role not named here (chart colours, the plain `red`...) stays legible.
pub fn colors(t: &Tokens) -> ThemeColor {
    let mut c = if t.dark { *ThemeColor::dark() } else { *ThemeColor::light() };
    let transparent = Hsla::transparent_black();
    // On a filled colour (accent, danger...) the text is the page's `--fg-on-accent`.
    let on_fill = t.fg_on_accent;
    let (step, link_step) = (t.step_lift, t.step_link);

    c.background = transparent;
    c.foreground = t.fg;
    c.border = t.line;
    c.input = t.line_strong;
    c.window_border = t.line;
    c.muted = t.bg_surface;
    c.muted_foreground = t.fg_faint;
    c.accent = t.bg_selected;
    c.accent_foreground = t.fg;
    c.popover = t.bg_surface;
    c.popover_foreground = t.fg;
    c.overlay = t.overlay_dim;
    c.caret = t.fg;
    c.ring = t.accent;
    c.selection = t.selection;
    c.drag_border = t.accent;
    c.drop_target = t.drop_target;

    c.primary = t.accent;
    c.primary_hover = t.accent.lighten(step);
    c.primary_active = t.accent.darken(step);
    c.primary_foreground = on_fill;
    c.secondary = transparent;
    c.secondary_hover = t.hover_fill;
    c.secondary_active = t.press_fill;
    c.secondary_foreground = t.fg;

    c.danger = t.danger;
    c.danger_hover = t.danger.lighten(step);
    c.danger_active = t.danger.darken(step);
    c.danger_foreground = on_fill;
    c.warning = t.attention;
    c.warning_hover = t.attention.lighten(step);
    c.warning_active = t.attention.darken(step);
    c.warning_foreground = on_fill;
    c.success = t.ok;
    c.success_hover = t.ok.lighten(step);
    c.success_active = t.ok.darken(step);
    c.success_foreground = on_fill;
    c.info = t.info;
    c.info_hover = t.info.lighten(step);
    c.info_active = t.info.darken(step);
    c.info_foreground = on_fill;

    c.link = t.ref_;
    c.link_hover = t.ref_.lighten(link_step);
    c.link_active = t.ref_.darken(link_step);

    c.button = transparent;
    c.button_hover = t.hover_fill;
    c.button_active = t.press_fill;
    c.button_foreground = t.fg;
    c.button_primary = c.primary;
    c.button_primary_hover = c.primary_hover;
    c.button_primary_active = c.primary_active;
    c.button_primary_foreground = on_fill;
    c.button_secondary = transparent;
    c.button_secondary_hover = t.hover_fill;
    c.button_secondary_active = t.press_fill;
    c.button_secondary_foreground = t.fg;
    c.button_danger = c.danger;
    c.button_danger_hover = c.danger_hover;
    c.button_danger_active = c.danger_active;
    c.button_danger_foreground = on_fill;
    c.button_warning = c.warning;
    c.button_warning_hover = c.warning_hover;
    c.button_warning_active = c.warning_active;
    c.button_warning_foreground = on_fill;
    c.button_success = c.success;
    c.button_success_hover = c.success_hover;
    c.button_success_active = c.success_active;
    c.button_success_foreground = on_fill;
    c.button_info = c.info;
    c.button_info_hover = c.info_hover;
    c.button_info_active = c.info_active;
    c.button_info_foreground = on_fill;

    c.sidebar = t.bg_chrome;
    c.sidebar_foreground = t.fg_muted;
    c.sidebar_border = t.line;
    c.sidebar_accent = t.bg_selected;
    c.sidebar_accent_foreground = t.fg;
    c.sidebar_primary = t.accent;
    c.sidebar_primary_foreground = on_fill;
    c.title_bar = t.bg_chrome;
    c.title_bar_border = t.line;
    c.status_bar = t.bg_chrome;
    c.status_bar_border = t.line;

    c.list = t.bg_sunken;
    c.list_even = t.bg_sunken;
    c.list_head = t.bg_chrome;
    c.list_hover = t.hover_fill;
    c.list_active = t.bg_selected;
    c.list_active_border = t.accent;
    c.table = t.bg_sunken;
    c.table_even = t.bg_sunken;
    c.table_head = t.bg_chrome;
    c.table_head_foreground = t.fg_faint;
    c.table_foot = t.bg_chrome;
    c.table_foot_foreground = t.fg_faint;
    c.table_hover = t.hover_fill;
    c.table_active = t.bg_selected;
    c.table_active_border = t.accent;
    c.table_row_border = t.line;

    c.tab_bar = t.bg_chrome;
    c.tab_bar_segmented = t.bg_surface;
    c.tab = transparent;
    c.tab_active = t.bg_sunken;
    c.tab_foreground = t.fg_faint;
    c.tab_active_foreground = t.fg;

    c.scrollbar = transparent;
    c.scrollbar_thumb = t.line_strong;
    c.scrollbar_thumb_hover = t.fg_fainter;
    c.skeleton = t.bg_surface;
    c.progress_bar = t.accent;
    c.slider_bar = t.accent;
    c.slider_thumb = t.fg;
    c.switch = t.line_strong;
    c.switch_thumb = t.fg;
    c.accordion = t.bg_sunken;
    c.group_box = t.bg_surface;
    c.group_box_foreground = t.fg;
    c.description_list_label = t.bg_surface;
    c.description_list_label_foreground = t.fg_faint;

    c.red = t.danger;
    c.green = t.ok;
    c.blue = t.ref_;
    c.yellow = t.attention;
    c.cyan = t.info;
    c.chart_1 = t.accent;
    c.chart_2 = t.ok;
    c.chart_3 = t.attention;
    c.chart_4 = t.info;
    c.chart_5 = t.changed;
    c.chart_grid = t.line;
    c
}

/// The kit's editor look for these tokens: the code area on `--bg-sunken` with the gutter on the
/// same ground, the text `--fg`, line numbers `--fg-faint` and the active line `--fill` (the
/// page's `.cm-editor` rules), and the syntax colours of the capture table (`theme/syntax`).
pub fn highlight(t: &Tokens) -> HighlightTheme {
    let mut theme = (*if t.dark { HighlightTheme::default_dark() } else { HighlightTheme::default_light() }).clone();
    theme.name = t.name.to_owned();
    let style = &mut theme.style;
    style.editor_background = Some(t.bg_sunken);
    style.editor_foreground = Some(t.fg);
    style.editor_gutter_background = Some(t.bg_sunken);
    style.editor_active_line = Some(t.fill);
    style.editor_line_number = Some(t.fg_faint);
    style.editor_active_line_number = Some(t.fg_muted);
    style.syntax = super::syntax::colours(t);
    theme
}


/// Puts `tokens` on gpui-kit's global theme (colours, faces, sizes, radii and
/// motion) and registers them as the [`Tokens`] global beside it: installing
/// another `Tokens` is how the theme switches. Call after `gpui_kit::init`
/// and, once, `fonts::install`.
///
/// `zoom` is the percent the whole window is drawn at. The kit's root sets the window's rem size
/// to the theme's font size on every frame, so the font size is where the zoom goes: a rem is
/// `fs_md` at 100% (`scale::REM`) and every view's size is a rem of its token.
///
/// The mode is set first and the colours in a second `Theme::update`: a mode
/// change reloads that mode's registered theme over any colour edited in the
/// same closure.
pub fn install(cx: &mut App, tokens: Tokens, zoom: u16) {
    let zoom = f32::from(zoom) / 100.0;
    Theme::change(if tokens.dark { ThemeMode::Dark } else { ThemeMode::Light }, None, cx);
    Theme::update(cx, |theme| {
        theme.colors = colors(&tokens);
        theme.font_family = tokens.font_ui.into();
        theme.mono_font_family = tokens.font_mono.into();
        theme.font_size = px(tokens.fs_md * zoom);
        theme.mono_font_size = px(tokens.type_mono.size * zoom);
        // `--r-sm` for a control, `--r-lg` for a dialog or a notice.
        theme.radius = px(tokens.r_sm * zoom);
        theme.radius_lg = px(tokens.r_lg * zoom);
        theme.motion.duration_fast = tokens.t_fast;
        // Flat surfaces with hairlines: depth comes from the line, not a shadow.
        theme.shadow = false;
        theme.highlight_theme = std::sync::Arc::new(highlight(&tokens));
    });
    cx.set_global(tokens);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::colour::Colour;
    use crate::theme::define::{Mode, Set, Theme};
    use crate::theme::derive;
    use crate::theme::themes::ledger;
    use gpui_kit::component::ActiveTheme as _;

    /// A light theme that is not Ledger in any palette entry, to see what an install changes.
    const OTHER: Theme = Theme {
        name: "other",
        mode: Mode::Light,
        palette: crate::theme::define::Palette {
            crust: Colour::hex(0xdce0e8),
            mantle: Colour::hex(0xe6e9ef),
            base: Colour::hex(0xeff1f5),
            surface0: Colour::hex(0xccd0da),
            surface1: Colour::hex(0xbcc0cc),
            overlay: Colour::hex(0x6c6f85),
            subtext: Colour::hex(0x5c5f77),
            text: Colour::hex(0x4c4f69),
            mauve: Colour::hex(0x7a33d6),
            blue: Colour::hex(0x1e5fd6),
            green: Colour::hex(0x2f7d1f),
            yellow: Colour::hex(0xa35200),
            red: Colour::hex(0xc20f37),
            peach: Colour::hex(0xa8420a),
            teal: Colour::hex(0x11766f),
            lilac: Colour::hex(0xdcd4f2),
            fg_on_accent: Colour::hex(0xffffff),
        },
        ansi: [Colour::hex(0x5c5f77); 16],
        sets: &[Set::FontUi("Other Sans"), Set::RSm(2.0), Set::RLg(3.0), Set::ControlH(32.0)],
    };

    #[test]
    fn kit_roles_carry_the_ledger_surfaces() {
        let t = ledger::tokens();
        let colors = colors(&t);
        assert_eq!(colors.background, Hsla::transparent_black());
        assert_eq!(colors.sidebar, t.bg_chrome);
        assert_eq!(colors.border, t.line);
        assert_eq!(colors.primary, t.accent);
        assert_eq!(colors.danger, t.danger);
    }

    #[test]
    fn hover_and_press_are_the_pages_foreground_mixes() {
        let t = ledger::tokens();
        assert!((t.hover_fill.a - 0.07).abs() < 1e-6);
        assert!((t.press_fill.a - 0.12).abs() < 1e-6);
        assert_eq!(colors(&t).button_hover, t.hover_fill);
    }

    #[gpui_kit::test]
    fn install_sets_the_global_theme(cx: &mut gpui_kit::TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            install(cx, ledger::tokens(), 100);
            assert_eq!(cx.theme().background, Hsla::transparent_black());
            assert!(cx.theme().is_dark());
            assert_eq!(cx.theme().font_family, "Inter");
            assert_eq!(cx.theme().mono_font_family, "JetBrains Mono");
            assert_eq!(cx.theme().font_size, px(13.0));
            assert_eq!(cx.theme().radius, px(6.0));
            assert_eq!(cx.theme().radius_lg, px(12.0));
        });
    }

    #[gpui_kit::test]
    fn installing_another_theme_changes_what_a_view_reads(cx: &mut gpui_kit::TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            install(cx, ledger::tokens(), 100);
            let before = *Tokens::global(cx);
            install(cx, derive::tokens(&OTHER), 100);
            let after = *Tokens::global(cx);
            assert_ne!(before.bg_sunken, after.bg_sunken);
            assert_ne!(before.fg, after.fg);
            assert_ne!(before.ansi[1], after.ansi[1]);
            assert_eq!(after.font_ui, "Other Sans");
            assert_eq!(after.control_h, 32.0);
            // What the theme leaves alone is `:root`'s, not Ledger's.
            assert_eq!(after.r_md, 10.0);
            // The kit's roles and sizes follow the same tokens.
            assert_eq!(cx.theme().background, Hsla::transparent_black());
            assert_eq!(cx.theme().radius, px(2.0));
            assert_eq!(cx.theme().radius_lg, px(3.0));
            assert_eq!(cx.theme().font_family, "Other Sans");
            assert!(!cx.theme().is_dark());
        });
    }
}
