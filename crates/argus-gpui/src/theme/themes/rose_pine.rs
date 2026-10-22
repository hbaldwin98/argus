//! Rose Pine as data: `:root[data-theme='rosePine']` of the stylesheet.

use crate::theme::colour::Colour;
use crate::theme::define::{Mode, Palette, Set, Theme};
use crate::theme::scale::{Layer, Mark, Type};

const fn hex(rgb: u32) -> Colour {
    Colour::hex(rgb)
}

const ROSE_PINE_POP: &[Layer] = &[
    Layer { dx: 0.0, dy: 12.0, blur: 32.0, spread: 0.0, colour: Colour::rgba(0, 0, 0, 0.35) },
];

/// Rose Pine: dusky violet surfaces, nothing near black, rose as the one warm accent.
pub const ROSE_PINE: Theme = Theme {
    name: "rosePine",
    mode: Mode::Dark,
    palette: Palette {
        crust: hex(0x191724),
        mantle: hex(0x1f1d2e),
        base: hex(0x191724),
        surface0: hex(0x26233a),
        surface1: hex(0x524f67),
        overlay: hex(0x908caa),
        subtext: hex(0x908caa),
        text: hex(0xe0def4),
        mauve: hex(0xebbcba),
        blue: hex(0xc4a7e7),
        green: hex(0x9ccfd8),
        yellow: hex(0xf6c177),
        red: hex(0xeb6f92),
        peach: hex(0xebbcba),
        teal: hex(0x3e8fb0),
        lilac: hex(0x26233a),
        fg_on_accent: hex(0x191724),
    },
    ansi: [
        hex(0x26233a),
        hex(0xeb6f92),
        hex(0x31748f),
        hex(0xf6c177),
        hex(0x9ccfd8),
        hex(0xc4a7e7),
        hex(0xebbcba),
        hex(0xe0def4),
        hex(0x6e6a86),
        hex(0xeb6f92),
        hex(0x31748f),
        hex(0xf6c177),
        hex(0x9ccfd8),
        hex(0xc4a7e7),
        hex(0xebbcba),
        hex(0xe0def4),
    ],
    sets: &[
        Set::FontUi("DM Sans"),
        Set::FontMono("DM Mono"),
        Set::RXs(6.0),
        Set::RSm(8.0),
        Set::RMd(12.0),
        Set::RLg(16.0),
        Set::ControlH(32.0),
        Set::ControlHSm(26.0),
        Set::IconBtn(30.0),
        Set::TypeDisplay(Type::new(650.0, 20.0, 1.3)),
        Set::TypeTitle(Type::new(600.0, 17.0, 1.3)),
        Set::TypeHeading(Type::new(600.0, 15.0, 1.35)),
        Set::TypeLabel(Type::new(600.0, 11.0, 1.4)),
        Set::ShadowPop(ROSE_PINE_POP),
        Set::MarkSel(Mark::None),
    ],
};
