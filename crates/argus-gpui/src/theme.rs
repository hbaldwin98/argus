//! Which colours the Ledger look uses, and how it and the page's tokens become gpui-kit's theme.
//!
//! gpui-kit's `Theme` has no role for `changed` or the `fainter` grey, so
//! [`Ledger`] keeps the whole palette as a global of our own and [`install`]
//! projects it onto gpui-kit's roles. Views read `cx.theme()` for what a kit
//! component also paints, and [`Ledger`] for the rest.

use gpui_kit::component::{Colorize as _, Theme, ThemeColor, ThemeMode};
use gpui_kit::{px, App, Global, Hsla, Rgba};

use crate::{fonts, tokens};

/// The palette, named for what Argus uses each colour for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ledger {
    pub background: Hsla,
    /// The sidebar and the header strip.
    pub chrome: Hsla,
    pub raised: Hsla,
    pub selected: Hsla,
    pub line: Hsla,
    pub line_strong: Hsla,
    pub foreground: Hsla,
    pub muted: Hsla,
    pub faint: Hsla,
    /// Below `faint`: line numbers, disabled text, the quietest mark.
    pub fainter: Hsla,
    pub accent: Hsla,
    pub warning: Hsla,
    /// A file or line changed in the working tree; `warning`'s yellow means
    /// "needs you", so a change gets its own orange.
    pub changed: Hsla,
    pub danger: Hsla,
    pub success: Hsla,
    pub link: Hsla,
    pub info: Hsla,
}

impl Global for Ledger {}

/// A `0xRRGGBB` literal as a colour.
fn hex(rgb: u32) -> Hsla {
    Rgba {
        r: ((rgb >> 16) & 0xff) as f32 / 255.0,
        g: ((rgb >> 8) & 0xff) as f32 / 255.0,
        b: (rgb & 0xff) as f32 / 255.0,
        a: 1.0,
    }
    .into()
}

impl Ledger {
    /// The user's palette.
    pub fn new() -> Self {
        Self {
            background: hex(0x08090a),
            chrome: hex(0x0f1011),
            raised: hex(0x141516),
            selected: hex(0x1e1f25),
            line: hex(0x23252a),
            line_strong: hex(0x34343a),
            foreground: hex(0xf7f8f8),
            muted: hex(0xb4bac3),
            faint: hex(0x8a8f98),
            fainter: hex(0x5c6068),
            accent: hex(0x828fff),
            warning: hex(0xf2c94c),
            changed: hex(0xf2994a),
            danger: hex(0xf07a7a),
            success: hex(0x4cb782),
            link: hex(0x4ea7fc),
            info: hex(0x26b5ce),
        }
    }

    pub fn global(cx: &App) -> &Self {
        cx.global::<Self>()
    }

    /// `--bg-raised`: what sits on a surface (a selected segment, a chip's
    /// card). Ledger leaves the page's mix, 55% of `--bg-surface` and 45% of
    /// `--fill`, which is `0x1b1c1f` over its greys.
    pub fn lifted(&self) -> Hsla {
        hex(0x1b1c1f)
    }

    /// `--fill`: what a row or an icon button lifts to under the pointer. The
    /// page's `--fill` is Ledger's `--surface0`, the hairline's own grey.
    pub fn fill(&self) -> Hsla {
        self.line
    }

    /// `.rfile.on:hover`: a lit row under the pointer, the selected colour
    /// with 6% of the text colour over it.
    pub fn selected_hover(&self) -> Hsla {
        self.selected.blend(self.foreground.opacity(0.06))
    }

    /// `.btn:hover`: the text colour at 7% over whatever is behind, so a
    /// hover reads on every surface the way the page's does.
    pub fn hover_fill(&self) -> Hsla {
        self.foreground.opacity(0.07)
    }

    /// `.btn:active`: 12%.
    pub fn press_fill(&self) -> Hsla {
        self.foreground.opacity(0.12)
    }

    /// `--ring-shadow`: the halo around a focused control.
    pub fn ring_halo(&self) -> Hsla {
        self.accent.opacity(tokens::focus::HALO_ALPHA)
    }

    /// gpui-kit's roles for this palette, starting from its dark set so any
    /// role not named here (chart colours, the plain `red`...) stays legible.
    ///
    /// Hover and active states are the base colour stepped lighter and
    /// darker, since the palette has one raised and one selected grey and a
    /// button needs three.
    pub fn colors(&self) -> ThemeColor {
        let mut c = *ThemeColor::dark();
        let transparent = Hsla::transparent_black();
        // On a filled colour (accent, danger...) the text is the window
        // background: dark on bright reads the same everywhere.
        let on_fill = self.background;

        c.background = self.background;
        c.foreground = self.foreground;
        c.border = self.line;
        c.input = self.line_strong;
        c.window_border = self.line;
        c.muted = self.raised;
        c.muted_foreground = self.faint;
        c.accent = self.selected;
        c.accent_foreground = self.foreground;
        c.popover = self.raised;
        c.popover_foreground = self.foreground;
        c.overlay = hex(0x000000).opacity(0.55);
        c.caret = self.foreground;
        c.ring = self.accent;
        c.selection = self.accent.opacity(0.30);
        c.drag_border = self.accent;
        c.drop_target = self.accent.opacity(0.18);

        c.primary = self.accent;
        c.primary_hover = self.accent.lighten(0.06);
        c.primary_active = self.accent.darken(0.06);
        c.primary_foreground = on_fill;
        c.secondary = transparent;
        c.secondary_hover = self.hover_fill();
        c.secondary_active = self.press_fill();
        c.secondary_foreground = self.foreground;

        c.danger = self.danger;
        c.danger_hover = self.danger.lighten(0.06);
        c.danger_active = self.danger.darken(0.06);
        c.danger_foreground = on_fill;
        c.warning = self.warning;
        c.warning_hover = self.warning.lighten(0.06);
        c.warning_active = self.warning.darken(0.06);
        c.warning_foreground = on_fill;
        c.success = self.success;
        c.success_hover = self.success.lighten(0.06);
        c.success_active = self.success.darken(0.06);
        c.success_foreground = on_fill;
        c.info = self.info;
        c.info_hover = self.info.lighten(0.06);
        c.info_active = self.info.darken(0.06);
        c.info_foreground = on_fill;

        c.link = self.link;
        c.link_hover = self.link.lighten(0.08);
        c.link_active = self.link.darken(0.08);

        c.button = transparent;
        c.button_hover = self.hover_fill();
        c.button_active = self.press_fill();
        c.button_foreground = self.foreground;
        c.button_primary = c.primary;
        c.button_primary_hover = c.primary_hover;
        c.button_primary_active = c.primary_active;
        c.button_primary_foreground = on_fill;
        c.button_secondary = transparent;
        c.button_secondary_hover = self.hover_fill();
        c.button_secondary_active = self.press_fill();
        c.button_secondary_foreground = self.foreground;
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

        c.sidebar = self.chrome;
        c.sidebar_foreground = self.muted;
        c.sidebar_border = self.line;
        c.sidebar_accent = self.selected;
        c.sidebar_accent_foreground = self.foreground;
        c.sidebar_primary = self.accent;
        c.sidebar_primary_foreground = on_fill;
        c.title_bar = self.chrome;
        c.title_bar_border = self.line;
        c.status_bar = self.chrome;
        c.status_bar_border = self.line;

        c.list = self.background;
        c.list_even = self.background;
        c.list_head = self.chrome;
        c.list_hover = self.hover_fill();
        c.list_active = self.selected;
        c.list_active_border = self.accent;
        c.table = self.background;
        c.table_even = self.background;
        c.table_head = self.chrome;
        c.table_head_foreground = self.faint;
        c.table_foot = self.chrome;
        c.table_foot_foreground = self.faint;
        c.table_hover = self.hover_fill();
        c.table_active = self.selected;
        c.table_active_border = self.accent;
        c.table_row_border = self.line;

        c.tab_bar = self.chrome;
        c.tab_bar_segmented = self.raised;
        c.tab = transparent;
        c.tab_active = self.background;
        c.tab_foreground = self.faint;
        c.tab_active_foreground = self.foreground;

        c.scrollbar = transparent;
        c.scrollbar_thumb = self.line_strong;
        c.scrollbar_thumb_hover = self.fainter;
        c.skeleton = self.raised;
        c.progress_bar = self.accent;
        c.slider_bar = self.accent;
        c.slider_thumb = self.foreground;
        c.switch = self.line_strong;
        c.switch_thumb = self.foreground;
        c.accordion = self.background;
        c.group_box = self.raised;
        c.group_box_foreground = self.foreground;
        c.description_list_label = self.raised;
        c.description_list_label_foreground = self.faint;

        c.red = self.danger;
        c.green = self.success;
        c.blue = self.link;
        c.yellow = self.warning;
        c.cyan = self.info;
        c.chart_1 = self.accent;
        c.chart_2 = self.success;
        c.chart_3 = self.warning;
        c.chart_4 = self.info;
        c.chart_5 = self.changed;
        c.chart_grid = self.line;
        c
    }
}

/// `.winbtn.shut:hover`: Windows' own close red and white, which no theme
/// recolours (the stylesheet writes them as literals), so they are not in the
/// palette.
pub fn window_close_hover() -> (Hsla, Hsla) {
    (hex(0xc42b1c), hex(0xffffff))
}

impl Default for Ledger {
    fn default() -> Self {
        Self::new()
    }
}

/// Registers the bundled fonts, puts the Ledger palette and the page's type,
/// radii and motion on gpui-kit's global theme, and registers [`Ledger`]
/// beside it. Call after `gpui_kit::init`.
///
/// The mode is set first and the colours in a second `Theme::update`: a mode
/// change reloads that mode's registered theme over any colour edited in the
/// same closure.
pub fn install(cx: &mut App) {
    fonts::install(cx);
    let ledger = Ledger::new();

    Theme::change(ThemeMode::Dark, None, cx);
    Theme::update(cx, |theme| {
        theme.colors = ledger.colors();
        theme.font_family = fonts::UI.into();
        theme.mono_font_family = fonts::MONO.into();
        theme.font_size = px(tokens::fs::MD);
        theme.mono_font_size = px(tokens::ty::MONO.size);
        // `--r-sm` for a control, `--r-lg` for a dialog or a notice.
        theme.radius = px(tokens::radius::SM);
        theme.radius_lg = px(tokens::radius::LG);
        theme.motion.duration_fast = tokens::motion::FAST;
        // Flat surfaces with hairlines: depth comes from the line, not a shadow.
        theme.shadow = false;
    });
    cx.set_global(ledger);
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::component::ActiveTheme as _;

    #[test]
    fn kit_roles_carry_the_ledger_surfaces() {
        let ledger = Ledger::new();
        let colors = ledger.colors();
        assert_eq!(colors.background, ledger.background);
        assert_eq!(colors.sidebar, ledger.chrome);
        assert_eq!(colors.border, ledger.line);
        assert_eq!(colors.primary, ledger.accent);
        assert_eq!(colors.danger, ledger.danger);
    }

    #[test]
    fn hex_reads_the_background_literal() {
        let rgba = hex(0x08090a).to_rgb();
        assert!((rgba.r - 8.0 / 255.0).abs() < 1e-3);
        assert!((rgba.b - 10.0 / 255.0).abs() < 1e-3);
    }

    #[test]
    fn hover_and_press_are_the_pages_foreground_mixes() {
        let ledger = Ledger::new();
        assert!((ledger.hover_fill().a - 0.07).abs() < 1e-6);
        assert!((ledger.press_fill().a - 0.12).abs() < 1e-6);
        assert_eq!(ledger.colors().button_hover, ledger.hover_fill());
    }

    #[gpui_kit::test]
    fn install_sets_the_global_theme(cx: &mut gpui_kit::TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            install(cx);
            assert_eq!(cx.theme().background, Ledger::global(cx).background);
            assert!(cx.theme().is_dark());
            assert_eq!(cx.theme().font_family, fonts::UI);
            assert_eq!(cx.theme().mono_font_family, fonts::MONO);
            assert_eq!(cx.theme().font_size, px(13.0));
            assert_eq!(cx.theme().radius, px(6.0));
            assert_eq!(cx.theme().radius_lg, px(12.0));
        });
    }
}
