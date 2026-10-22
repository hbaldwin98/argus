//! Flexoki as data: `:root[data-theme='flexoki']` of the stylesheet.

use crate::theme::colour::Colour;
use crate::theme::define::{Mode, Palette, Set, Theme};
use crate::theme::scale::{Layer, Mark, Type};

const fn hex(rgb: u32) -> Colour {
    Colour::hex(rgb)
}

const FLEXOKI_POP: &[Layer] = &[
    Layer { dx: 0.0, dy: 4.0, blur: 16.0, spread: 0.0, colour: Colour::rgba(0, 0, 0, 0.4) },
];

/// Steph Ango's Flexoki dark: warm ink on black paper, cyan for what you can act on.
pub const FLEXOKI: Theme = Theme {
    name: "flexoki",
    mode: Mode::Dark,
    palette: Palette {
        crust: hex(0x100f0f),
        mantle: hex(0x1c1b1a),
        base: hex(0x100f0f),
        surface0: hex(0x282726),
        surface1: hex(0x403e3c),
        overlay: hex(0x878580),
        subtext: hex(0xb7b5ac),
        text: hex(0xcecdc3),
        mauve: hex(0x3aa99f),
        blue: hex(0x66a0c8),
        green: hex(0x879a39),
        yellow: hex(0xd0a215),
        red: hex(0xe8705f),
        peach: hex(0xda702c),
        teal: hex(0x8b7ec8),
        lilac: hex(0x343331),
        fg_on_accent: hex(0x100f0f),
    },
    ansi: [
        hex(0x282726),
        hex(0xd14d41),
        hex(0x879a39),
        hex(0xd0a215),
        hex(0x4385be),
        hex(0xce5d97),
        hex(0x3aa99f),
        hex(0xb7b5ac),
        hex(0x575653),
        hex(0xe8705f),
        hex(0xa0af54),
        hex(0xdfb431),
        hex(0x66a0c8),
        hex(0xe47da8),
        hex(0x5abdac),
        hex(0xcecdc3),
    ],
    sets: &[
        Set::FontUi("Instrument Sans"),
        Set::FontMono("IBM Plex Mono"),
        Set::RXs(3.0),
        Set::RSm(4.0),
        Set::RMd(6.0),
        Set::RLg(8.0),
        Set::ControlH(30.0),
        Set::ControlHSm(24.0),
        Set::IconBtn(28.0),
        Set::TypeDisplay(Type::new(600.0, 20.0, 1.3)),
        Set::TypeTitle(Type::new(600.0, 17.0, 1.3)),
        Set::TypeHeading(Type::new(600.0, 15.0, 1.35)),
        Set::TypeLabel(Type::new(600.0, 11.0, 1.4)),
        Set::ShadowPop(FLEXOKI_POP),
        Set::MarkSel(Mark::Hairline),
    ],
};
