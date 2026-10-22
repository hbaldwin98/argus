//! Argus Night as data: `:root[data-theme='night']` of the stylesheet.

use crate::theme::colour::Colour;
use crate::theme::define::{Mode, Palette, Set, Theme};
use crate::theme::scale::Layer;

const fn hex(rgb: u32) -> Colour {
    Colour::hex(rgb)
}

const NIGHT_POP: &[Layer] = &[
    Layer { dx: 0.0, dy: 12.0, blur: 32.0, spread: 0.0, colour: Colour::rgba(0, 0, 0, 0.5) },
    Layer { dx: 0.0, dy: 0.0, blur: 0.0, spread: 1.0, colour: Colour::rgba(152, 192, 239, 0.06) },
];

const NIGHT_DIALOG: &[Layer] = &[
    Layer { dx: 0.0, dy: 28.0, blur: 72.0, spread: 0.0, colour: Colour::rgba(0, 0, 0, 0.6) },
    Layer { dx: 0.0, dy: 0.0, blur: 48.0, spread: 0.0, colour: Colour::rgba(123, 143, 240, 0.08) },
];

/// Argus's own: a cool navy ground lit by one periwinkle accent. Its glow, the primary button's gradient and the empty stages' atmosphere are not drawn: the token space holds colours, sizes and shadows only.
pub const NIGHT: Theme = Theme {
    name: "night",
    mode: Mode::Dark,
    palette: Palette {
        crust: hex(0x06070d),
        mantle: hex(0x0a0c16),
        base: hex(0x0d1020),
        surface0: hex(0x161a2e),
        surface1: hex(0x272e52),
        overlay: hex(0x7a86a0),
        subtext: hex(0x9aa7bd),
        text: hex(0xe6edf8),
        mauve: hex(0x98c0ef),
        blue: hex(0x9db0ff),
        green: hex(0x5fd4a8),
        yellow: hex(0xf2c46d),
        red: hex(0xf27a86),
        peach: hex(0xf0a070),
        teal: hex(0x6fd3e0),
        lilac: hex(0x1a1f3d),
        fg_on_accent: hex(0x07080f),
    },
    ansi: [
        hex(0x272e52),
        hex(0xf27a86),
        hex(0x5fd4a8),
        hex(0xf2c46d),
        hex(0x8fb0ff),
        hex(0xc7a6ff),
        hex(0x6fd3e0),
        hex(0xc3cee0),
        hex(0x4a5577),
        hex(0xff9aa5),
        hex(0x8fe3c4),
        hex(0xf7d796),
        hex(0xb0c8ff),
        hex(0xdcc6ff),
        hex(0x9be4ee),
        hex(0xe6edf8),
    ],
    sets: &[
        Set::BgRaised(hex(0x11142a)),
        Set::Line(Colour::rgba(152, 192, 239, 0.12)),
        Set::LineStrong(Colour::rgba(152, 192, 239, 0.24)),
        Set::Ring(hex(0x7b8ff0)),
        Set::Scrim(Colour::rgba(4, 5, 10, 0.64)),
        Set::ShadowPop(NIGHT_POP),
        Set::ShadowDialog(NIGHT_DIALOG),
    ],
};
