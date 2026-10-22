//! How a [`Theme`]'s palette and overrides become the whole token space, by the page's own formulas.
//!
//! The semantic names are the palette's under another name (`--bg-chrome` is
//! `var(--mantle)`), a theme may pin a few of them, and the rest are
//! `color-mix` of those. The order here is the stylesheet's: alias the
//! palette, lay the theme's overrides over the aliases and `:root`'s scale,
//! then derive what is left from the result, so a theme that pins `--fill`
//! changes `--bg-raised` with it, as it does in the browser.

use gpui_kit::Hsla;

use super::colour::{mix, wash, Colour};
use super::define::{Mode, Set, Theme};
use super::scale::{Mark, Scale};
use super::Tokens;

/// `:root`'s `--scrim`.
const ROOT_SCRIM: Colour = Colour::rgba(17, 17, 27, 0.62);
const WIN_CLOSE_BG: Colour = Colour::hex(0xc42b1c);
const WIN_CLOSE_FG: Colour = Colour::hex(0xffffff);

/// The token space of `theme`.
pub fn tokens(theme: &Theme) -> Tokens {
    let p = &theme.palette;
    let h = |c: Colour| c.hsla();

    let mut scale = Scale::root();
    let (mut fill, mut line, mut line_strong) = (p.surface0, p.surface0, p.surface1);
    let (mut bg_raised, mut ring, mut scrim, mut fg_fainter) = (None, None, ROOT_SCRIM, None);
    for set in theme.sets {
        match *set {
            Set::Fill(c) => fill = c,
            Set::Line(c) => line = c,
            Set::LineStrong(c) => line_strong = c,
            Set::BgRaised(c) => bg_raised = Some(c),
            Set::Ring(c) => ring = Some(c),
            Set::Scrim(c) => scrim = c,
            Set::FgFainter(c) => fg_fainter = Some(c),
            _ => scale.set(set),
        }
    }

    let (bg_sunken, bg_surface) = (h(p.crust), h(p.base));
    let fill = h(fill);
    let fg = h(p.text);
    let fg_faint = h(p.overlay);
    let accent = h(p.mauve);
    let ring = ring.map_or(accent, h);
    let bg_selected = h(p.lilac);
    let line_strong = h(line_strong);

    Tokens {
        name: theme.name,
        dark: theme.mode == Mode::Dark,

        crust: h(p.crust),
        mantle: h(p.mantle),
        base: h(p.base),
        surface0: h(p.surface0),
        surface1: h(p.surface1),
        overlay: h(p.overlay),
        subtext: h(p.subtext),
        text: h(p.text),
        mauve: h(p.mauve),
        blue: h(p.blue),
        green: h(p.green),
        yellow: h(p.yellow),
        red: h(p.red),
        peach: h(p.peach),
        teal: h(p.teal),
        lilac: h(p.lilac),
        fg_on_accent: h(p.fg_on_accent),
        ansi: theme.ansi.map(h),

        bg_sunken,
        bg_chrome: h(p.mantle),
        bg_surface,
        bg_selected,
        bg_raised: bg_raised.map_or_else(|| mix(bg_surface, scale.share_raised, fill), h),
        fill,
        line: h(line),
        line_strong,
        fg,
        fg_muted: h(p.subtext),
        fg_faint,
        fg_fainter: fg_fainter.map_or_else(|| mix(fg_faint, scale.share_fainter, bg_sunken), h),
        fg_disabled: h(p.surface1),
        accent,
        ref_: h(p.blue),
        ok: h(p.green),
        attention: h(p.yellow),
        danger: h(p.red),
        changed: h(p.peach),
        info: h(p.teal),
        ring,
        scrim: h(scrim),
        ring_halo: wash(ring, scale.ring_halo_share),
        // Only a bar is lit by the ring; the other marks, and the page's cards, take the hairline's colour.
        mark_sel_edge: if scale.mark_sel == Mark::Bar { ring } else { line_strong },

        hover_fill: wash(fg, scale.share_hover),
        press_fill: wash(fg, scale.share_press),
        selected_hover: mix(bg_selected, 1.0 - scale.share_selected_hover, fg),
        selection: wash(accent, scale.share_selection),
        drop_target: wash(accent, scale.share_drop_target),
        overlay_dim: wash(Hsla::black(), scale.share_overlay_dim),
        win_close_bg: h(WIN_CLOSE_BG),
        win_close_fg: h(WIN_CLOSE_FG),

        scale,
    }
}
