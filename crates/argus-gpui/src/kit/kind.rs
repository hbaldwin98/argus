//! What the outline puts in a row's icon slot (`project/Kit.dc.html`, "Outline"): a 16px tinted
//! square holding the kind's letter, never a text label that runs into the name, and the 3px bar
//! the outline's Changes list uses in the same slot.

use gpui_kit::{div, prelude::*, Div, Hsla, SharedString};

use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// Which tint a kind wears, by what it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Type,
    Function,
    Implementation,
    Constant,
    Module,
    Other,
}

impl Kind {
    /// The kind of core's outline entry (`fn`, `type`, `iface`, `mod`, `macro`, `const`, `def`).
    pub fn of(kind: &str) -> Self {
        match kind {
            "type" | "iface" | "struct" | "enum" | "trait" => Kind::Type,
            "fn" | "def" | "method" | "macro" => Kind::Function,
            "impl" => Kind::Implementation,
            "const" | "static" => Kind::Constant,
            "mod" => Kind::Module,
            _ => Kind::Other,
        }
    }

    /// The letter a square holds: the first of the kind, lower-case for what is called and
    /// upper-case for what is declared, as the board's `S`, `f`, `i`.
    pub fn letter(self, kind: &str) -> String {
        let first = kind.chars().next().map(|c| if self == Kind::Function || self == Kind::Implementation { c.to_ascii_lowercase() } else { c.to_ascii_uppercase() });
        first.map(String::from).unwrap_or_default()
    }

    /// The tint, from the theme's palette roles so every theme carries it.
    /// A function takes the accent: `lilac` is the selected surface, which is a dark wash in a dark theme and
    /// left the letter unreadable.
    pub fn tint(self, tokens: &Tokens) -> Hsla {
        match self {
            Kind::Type => tokens.blue,
            Kind::Function => tokens.accent,
            Kind::Implementation => tokens.ok,
            Kind::Constant => tokens.peach,
            Kind::Module => tokens.teal,
            Kind::Other => tokens.fg_muted,
        }
    }
}

/// `.sym`: the 16px square, its fill the tint at 16% and the letter the tint in mono 10px.
pub fn square(letter: impl Into<SharedString>, kind: Kind, tokens: &Tokens) -> Div {
    let tint = kind.tint(tokens);
    div()
        .flex_none()
        .size(rem(tokens.sp_16))
        .flex()
        .items_center()
        .justify_center()
        .rounded(rem(tokens.r_xs))
        .bg(tint.opacity(0.16))
        .text_color(tint)
        .font_family(SharedString::from(tokens.font_mono))
        .text_size(rem(10.0))
        .weighted(tokens.weight_semibold)
        .child(letter.into())
}

/// `.bar`: the 3px by 14px colour bar of a change in the outline's Changes list.
pub fn bar(colour: Hsla) -> Div {
    div().flex_none().w(rem(3.0)).h(rem(14.0)).rounded(rem(2.0)).bg(colour)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kit::host;
    use gpui_kit::{AnyElement, TestAppContext};

    #[test]
    fn core_kinds_map_to_a_tint_and_an_unknown_one_to_the_quiet_one() {
        assert_eq!(Kind::of("fn"), Kind::Function);
        assert_eq!(Kind::of("iface"), Kind::Type);
        assert_eq!(Kind::of("mod"), Kind::Module);
        assert_eq!(Kind::of("whatever"), Kind::Other);
        assert_eq!(Kind::Function.letter("fn"), "f");
        assert_eq!(Kind::Type.letter("type"), "T");
        assert_eq!(Kind::Other.letter(""), "");
    }

    #[test]
    fn each_kind_has_its_own_tint_in_the_default_theme() {
        let tokens = crate::theme::themes::ledger::tokens();
        let tints: Vec<_> = [Kind::Type, Kind::Function, Kind::Implementation, Kind::Constant, Kind::Module].map(|k| k.tint(&tokens)).to_vec();
        for (i, a) in tints.iter().enumerate() {
            assert!(tints.iter().skip(i + 1).all(|b| a != b), "tints are distinct");
        }
    }

    /// Kit.dc.html: the square is 16px, the bar 3 by 14, so the icon slot never grows.
    #[gpui_kit::test]
    fn the_square_is_16px_and_the_bar_3_by_14(cx: &mut TestAppContext) {
        use gpui_kit::TestSupportExt as _;
        let handle = host::open(cx, 200.0, |tokens| {
            gpui_kit::component::h_flex()
                .gap(rem(8.0))
                .child(square("S", Kind::Type, tokens).id("sq").test_support())
                .child(bar(tokens.ok).id("bar").test_support())
                .into_any_element() as AnyElement
        });
        let (_, _, w, h) = host::bounds(cx, handle, "sq");
        assert_eq!((w, h), (16.0, 16.0));
        let (_, _, w, h) = host::bounds(cx, handle, "bar");
        assert_eq!((w, h), (3.0, 14.0));
    }
}
