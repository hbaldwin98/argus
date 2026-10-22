//! Kanagawa as data: `:root[data-theme='kanagawa']` of the stylesheet.

use crate::theme::colour::Colour;
use crate::theme::define::{Mode, Palette, Set, Theme};
use crate::theme::scale::{Layer, Mark, Type};

const fn hex(rgb: u32) -> Colour {
    Colour::hex(rgb)
}

const KANAGAWA_POP: &[Layer] = &[
    Layer { dx: 0.0, dy: 6.0, blur: 20.0, spread: 0.0, colour: Colour::rgba(0, 0, 0, 0.45) },
];

/// Kanagawa Wave: sumi ink greys, aged-paper text, a wave-blue selection.
pub const KANAGAWA: Theme = Theme {
    name: "kanagawa",
    mode: Mode::Dark,
    palette: Palette {
        crust: hex(0x16161d),
        mantle: hex(0x181820),
        base: hex(0x1f1f28),
        surface0: hex(0x2a2a37),
        surface1: hex(0x363646),
        overlay: hex(0x938aa9),
        subtext: hex(0xc8c093),
        text: hex(0xdcd7ba),
        mauve: hex(0x7e9cd8),
        blue: hex(0x7fb4ca),
        green: hex(0x98bb6c),
        yellow: hex(0xe6c384),
        red: hex(0xff5d62),
        peach: hex(0xffa066),
        teal: hex(0x7aa89f),
        lilac: hex(0x223249),
        fg_on_accent: hex(0x16161d),
    },
    ansi: [
        hex(0x16161d),
        hex(0xc34043),
        hex(0x76946a),
        hex(0xc0a36e),
        hex(0x7e9cd8),
        hex(0x957fb8),
        hex(0x6a9589),
        hex(0xc8c093),
        hex(0x727169),
        hex(0xe82424),
        hex(0x98bb6c),
        hex(0xe6c384),
        hex(0x7fb4ca),
        hex(0x938aa9),
        hex(0x7aa89f),
        hex(0xdcd7ba),
    ],
    sets: &[
        Set::FontUi("Noto Sans"),
        Set::RXs(4.0),
        Set::RSm(5.0),
        Set::RMd(6.0),
        Set::RLg(8.0),
        Set::ControlH(28.0),
        Set::ControlHSm(24.0),
        Set::IconBtn(28.0),
        Set::TypeDisplay(Type::new(600.0, 20.0, 1.3)),
        Set::TypeTitle(Type::new(600.0, 17.0, 1.3)),
        Set::TypeHeading(Type::new(600.0, 15.0, 1.35)),
        Set::TypeLabel(Type::new(600.0, 11.0, 1.4)),
        Set::ShadowPop(KANAGAWA_POP),
        Set::MarkSel(Mark::None),
    ],
};
