//! What a theme is: data in the page's own names, a base palette, the sixteen ANSI colours and the overrides it lays over `:root`.
//!
//! A `:root[data-theme='x']` block of `style.css` becomes one [`Theme`]
//! constant: its `--crust ... --fg-on-accent` are the [`Palette`], its
//! `--ansi-0..15` the `ansi`, and every other declaration in its blocks (a
//! radius, a type shorthand, a shadow, a colour role it pins) one [`Set`]. A
//! theme holds no derived colour and no default: those are
//! [`super::derive`]'s and [`super::scale`]'s.

use super::colour::Colour;
use super::scale::{Layer, Mark, Type};

/// `color-scheme`, which picks the kit's base roles for any role a theme does not name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Dark,
    Light,
}

/// The page's palette layer, `--crust` to `--fg-on-accent`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub crust: Colour,
    pub mantle: Colour,
    pub base: Colour,
    pub surface0: Colour,
    pub surface1: Colour,
    pub overlay: Colour,
    pub subtext: Colour,
    pub text: Colour,
    pub mauve: Colour,
    pub blue: Colour,
    pub green: Colour,
    pub yellow: Colour,
    pub red: Colour,
    pub peach: Colour,
    pub teal: Colour,
    pub lilac: Colour,
    pub fg_on_accent: Colour,
}

/// One declaration a theme makes beyond its palette, named for the variable it sets.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Set {
    // Semantic colour roles a theme pins instead of taking the page's formula
    // (`--bg-raised`, `--fill`, `--line`...). `FgFainter` is the port's own
    // role, one step under `--fg-faint`, which the page has no variable for.
    BgRaised(Colour),
    Fill(Colour),
    Line(Colour),
    LineStrong(Colour),
    Ring(Colour),
    Scrim(Colour),
    FgFainter(Colour),
    // The faces: the first family of `--font-ui-bundled` and `--font-mono-bundled`.
    FontUi(&'static str),
    FontMono(&'static str),
    RXs(f32),
    RSm(f32),
    RMd(f32),
    RLg(f32),
    RPill(f32),
    ControlH(f32),
    ControlHSm(f32),
    IconBtn(f32),
    TypeDisplay(Type),
    TypeHero(Type),
    TypeTitle(Type),
    TypeHeading(Type),
    TypeBody(Type),
    TypeSmall(Type),
    TypeLabel(Type),
    TypeMono(Type),
    ShadowPop(&'static [Layer]),
    ShadowDialog(&'static [Layer]),
    MarkSel(Mark),
}

/// A theme, as data.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
    /// The `data-theme` value.
    pub name: &'static str,
    pub mode: Mode,
    pub palette: Palette,
    /// `--ansi-0` to `--ansi-15`.
    pub ansi: [Colour; 16],
    pub sets: &'static [Set],
}
