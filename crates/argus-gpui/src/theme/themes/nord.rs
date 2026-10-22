//! Nord as data: `:root[data-theme='nord']` of the stylesheet.

use crate::theme::colour::Colour;
use crate::theme::define::{Mode, Palette, Set, Theme};
use crate::theme::scale::{Layer, Mark, Type};

const fn hex(rgb: u32) -> Colour {
    Colour::hex(rgb)
}

const NORD_POP: &[Layer] = &[
    Layer { dx: 0.0, dy: 8.0, blur: 24.0, spread: 0.0, colour: Colour::rgba(0, 0, 0, 0.3) },
];

/// Nord: Polar Night greys, Snow Storm text, Frost blue as the accent.
pub const NORD: Theme = Theme {
    name: "nord",
    mode: Mode::Dark,
    palette: Palette {
        crust: hex(0x2e3440),
        mantle: hex(0x3b4252),
        base: hex(0x2e3440),
        surface0: hex(0x434c5e),
        surface1: hex(0x4c566a),
        overlay: hex(0xd8dee9),
        subtext: hex(0xe5e9f0),
        text: hex(0xeceff4),
        mauve: hex(0x88c0d0),
        blue: hex(0x8fbcbb),
        green: hex(0xa3be8c),
        yellow: hex(0xebcb8b),
        red: hex(0xbf616a),
        peach: hex(0xd08770),
        teal: hex(0xb48ead),
        lilac: hex(0x4c566a),
        fg_on_accent: hex(0x2e3440),
    },
    ansi: [
        hex(0x3b4252),
        hex(0xbf616a),
        hex(0xa3be8c),
        hex(0xebcb8b),
        hex(0x81a1c1),
        hex(0xb48ead),
        hex(0x88c0d0),
        hex(0xe5e9f0),
        hex(0x4c566a),
        hex(0xbf616a),
        hex(0xa3be8c),
        hex(0xebcb8b),
        hex(0x81a1c1),
        hex(0xb48ead),
        hex(0x8fbcbb),
        hex(0xeceff4),
    ],
    sets: &[
        Set::FontUi("Rubik"),
        Set::FontMono("Source Code Pro"),
        Set::RXs(4.0),
        Set::RSm(4.0),
        Set::RMd(6.0),
        Set::RLg(8.0),
        Set::ControlH(30.0),
        Set::ControlHSm(24.0),
        Set::IconBtn(28.0),
        Set::TypeDisplay(Type::new(500.0, 20.0, 1.3)),
        Set::TypeTitle(Type::new(500.0, 17.0, 1.3)),
        Set::TypeHeading(Type::new(500.0, 15.0, 1.35)),
        Set::TypeLabel(Type::new(500.0, 11.0, 1.4)),
        Set::ShadowPop(NORD_POP),
        Set::MarkSel(Mark::None),
    ],
};
