//! Ledger as data: `:root[data-theme='ledger']` of the stylesheet.
//!
//! Near-black greys, one indigo, hairlines at a single step (after Linear).
//! Its palette and `--ansi-N` are the blocks of the same name; its sets are
//! the "Shape per theme" block, the radii, control heights, heading weights
//! (Ledger sits between the stock weights, 590 and 510, which the variable
//! Inter draws as itself), `--shadow-pop` and the hairline selection mark.

use crate::theme::colour::Colour;
use crate::theme::define::{Mode, Palette, Set, Theme};
use crate::theme::derive;
use crate::theme::scale::{Layer, Mark, Type};
use crate::theme::Tokens;

const fn hex(rgb: u32) -> Colour {
    Colour::hex(rgb)
}

const POP: &[Layer] = &[Layer::drop(8.0, 24.0, 0.5)];

pub const THEME: Theme = Theme {
    name: "ledger",
    mode: Mode::Dark,
    palette: Palette {
        crust: hex(0x08090a),
        mantle: hex(0x0f1011),
        base: hex(0x141516),
        surface0: hex(0x23252a),
        surface1: hex(0x34343a),
        overlay: hex(0x8a8f98),
        subtext: hex(0xb4bac3),
        text: hex(0xf7f8f8),
        mauve: hex(0x828fff),
        blue: hex(0x4ea7fc),
        green: hex(0x4cb782),
        yellow: hex(0xf2c94c),
        red: hex(0xf07a7a),
        peach: hex(0xf2994a),
        teal: hex(0x26b5ce),
        lilac: hex(0x1e1f25),
        fg_on_accent: hex(0x08090a),
    },
    ansi: [
        hex(0x23252a),
        hex(0xeb5757),
        hex(0x4cb782),
        hex(0xf2c94c),
        hex(0x4ea7fc),
        hex(0x828fff),
        hex(0x26b5ce),
        hex(0xd0d6e0),
        hex(0x62666d),
        hex(0xf28b8b),
        hex(0x7fd1a6),
        hex(0xf7d774),
        hex(0x7ab8ff),
        hex(0xa3acff),
        hex(0x5fd0e3),
        hex(0xf7f8f8),
    ],
    sets: &[
        Set::FontUi("Inter"),
        Set::RXs(4.0),
        Set::RSm(6.0),
        Set::RMd(8.0),
        Set::RLg(12.0),
        Set::ControlH(28.0),
        Set::ControlHSm(24.0),
        Set::IconBtn(28.0),
        Set::TypeDisplay(Type::new(590.0, 20.0, 1.3)),
        Set::TypeTitle(Type::new(590.0, 17.0, 1.3)),
        Set::TypeHeading(Type::new(510.0, 15.0, 1.35)),
        Set::TypeLabel(Type::new(510.0, 11.0, 1.4)),
        Set::ShadowPop(POP),
        Set::MarkSel(Mark::Hairline),
        // The port's own role: no page variable sits between `--fg-faint` and `--ansi-8`.
        Set::FgFainter(hex(0x5c6068)),
    ],
};

/// Ledger's token space: the fixed theme the tests of a view's look are written in.
pub fn tokens() -> Tokens {
    derive::tokens(&THEME)
}
