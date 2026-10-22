//! Carbon as data: `:root[data-theme='carbon']` of the stylesheet.

use crate::theme::colour::Colour;
use crate::theme::define::{Mode, Palette, Set, Theme};
use crate::theme::scale::{Layer, Mark, Type};

const fn hex(rgb: u32) -> Colour {
    Colour::hex(rgb)
}

const CARBON_POP: &[Layer] = &[
    Layer { dx: 0.0, dy: 2.0, blur: 6.0, spread: 0.0, colour: Colour::rgba(0, 0, 0, 0.3) },
];

const CARBON_DIALOG: &[Layer] = &[
    Layer { dx: 0.0, dy: 2.0, blur: 6.0, spread: 0.0, colour: Colour::rgba(0, 0, 0, 0.3) },
];

/// IBM Carbon's Gray 100: neutral greys, Carbon blue, square and sober.
pub const CARBON: Theme = Theme {
    name: "carbon",
    mode: Mode::Dark,
    palette: Palette {
        crust: hex(0x000000),
        mantle: hex(0x161616),
        base: hex(0x161616),
        surface0: hex(0x393939),
        surface1: hex(0x525252),
        overlay: hex(0x8d8d8d),
        subtext: hex(0xc6c6c6),
        text: hex(0xf4f4f4),
        mauve: hex(0x78a9ff),
        blue: hex(0xa6c8ff),
        green: hex(0x42be65),
        yellow: hex(0xf1c21b),
        red: hex(0xff8389),
        peach: hex(0xff832b),
        teal: hex(0x3ddbd9),
        lilac: hex(0x002d9c),
        fg_on_accent: hex(0x161616),
    },
    ansi: [
        hex(0x393939),
        hex(0xfa4d56),
        hex(0x42be65),
        hex(0xf1c21b),
        hex(0x4589ff),
        hex(0xbe95ff),
        hex(0x33b1ff),
        hex(0xc6c6c6),
        hex(0x6f6f6f),
        hex(0xff8389),
        hex(0x6fdc8c),
        hex(0xfddc69),
        hex(0x78a9ff),
        hex(0xd4bbff),
        hex(0x82cfff),
        hex(0xf4f4f4),
    ],
    sets: &[
        Set::FontUi("IBM Plex Sans"),
        Set::FontMono("IBM Plex Mono"),
        Set::RXs(0.0),
        Set::RSm(0.0),
        Set::RMd(0.0),
        Set::RLg(0.0),
        Set::RPill(999.0),
        Set::ControlH(32.0),
        Set::ControlHSm(24.0),
        Set::IconBtn(32.0),
        Set::TypeDisplay(Type::new(400.0, 20.0, 1.3)),
        Set::TypeTitle(Type::new(400.0, 17.0, 1.3)),
        Set::TypeHeading(Type::new(600.0, 15.0, 1.35)),
        Set::TypeLabel(Type::new(400.0, 11.0, 1.4)),
        Set::ShadowPop(CARBON_POP),
        Set::ShadowDialog(CARBON_DIALOG),
        Set::MarkSel(Mark::Bar),
    ],
};
