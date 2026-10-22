//! Radix as data: `:root[data-theme='radix']` of the stylesheet.

use crate::theme::colour::Colour;
use crate::theme::define::{Mode, Palette, Set, Theme};
use crate::theme::scale::{Layer, Mark, Type};

const fn hex(rgb: u32) -> Colour {
    Colour::hex(rgb)
}

const RADIX_POP: &[Layer] = &[
    Layer { dx: 0.0, dy: 0.0, blur: 0.0, spread: 1.0, colour: hex(0x363a3f) },
    Layer { dx: 0.0, dy: 8.0, blur: 40.0, spread: 0.0, colour: Colour::rgba(0, 0, 0, 0.4) },
];

/// Radix Colors' slate and blue, each step in the role its scale gives it.
pub const RADIX: Theme = Theme {
    name: "radix",
    mode: Mode::Dark,
    palette: Palette {
        crust: hex(0x111113),
        mantle: hex(0x18191b),
        base: hex(0x111113),
        surface0: hex(0x363a3f),
        surface1: hex(0x43484e),
        overlay: hex(0x9a9fa7),
        subtext: hex(0xb0b4ba),
        text: hex(0xedeef0),
        mauve: hex(0x70b8ff),
        blue: hex(0x4ccce6),
        green: hex(0x71d083),
        yellow: hex(0xffca16),
        red: hex(0xff9592),
        peach: hex(0xff801f),
        teal: hex(0x0bd8b6),
        lilac: hex(0x0d2847),
        fg_on_accent: hex(0x111113),
    },
    ansi: [
        hex(0x2e3135),
        hex(0xe5484d),
        hex(0x46a758),
        hex(0xffc53d),
        hex(0x0090ff),
        hex(0x8e4ec6),
        hex(0x00a2c7),
        hex(0xb0b4ba),
        hex(0x5a6169),
        hex(0xff9592),
        hex(0x71d083),
        hex(0xffca16),
        hex(0x70b8ff),
        hex(0xd19dff),
        hex(0x4ccce6),
        hex(0xedeef0),
    ],
    sets: &[
        Set::Fill(hex(0x272a2d)),
        Set::FontUi("Inter"),
        Set::RXs(4.0),
        Set::RSm(6.0),
        Set::RMd(8.0),
        Set::RLg(12.0),
        Set::ControlH(32.0),
        Set::ControlHSm(24.0),
        Set::IconBtn(32.0),
        Set::TypeDisplay(Type::new(700.0, 20.0, 1.3)),
        Set::TypeTitle(Type::new(700.0, 17.0, 1.3)),
        Set::TypeHeading(Type::new(500.0, 15.0, 1.35)),
        Set::TypeLabel(Type::new(500.0, 11.0, 1.4)),
        Set::ShadowPop(RADIX_POP),
        Set::MarkSel(Mark::Hairline),
    ],
};
