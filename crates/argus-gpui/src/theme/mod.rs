//! The one design token space the window draws with: every colour and every number, under the page's own names.
//!
//! [`Tokens`] mirrors the stylesheet's two layers, so importing a theme is
//! filling in the same names: the palette layer (`--crust`...), the sixteen
//! `--ansi-N`, the semantic roles derived from them (`--bg-chrome`, `--fg`,
//! `--line`...), and [`Scale`] for everything that is a size, type, shadow or
//! time. A view reads a `Tokens` and nothing else; the active one is a GPUI
//! global, and installing another changes what every view reads.

pub mod colour;
pub mod define;
pub mod derive;
pub mod kit;
pub mod ledger;
pub mod scale;
#[cfg(test)]
mod stylesheet;

use std::ops::Deref;

use gpui_kit::{App, Global, Hsla};

use self::scale::Scale;

/// Every token of the active theme. The scale's fields are reached through
/// `Deref`, so a view says `tokens.fs_md` as the stylesheet says `var(--fs-md)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub name: &'static str,
    pub dark: bool,

    // The palette layer.
    pub crust: Hsla,
    pub mantle: Hsla,
    pub base: Hsla,
    pub surface0: Hsla,
    pub surface1: Hsla,
    pub overlay: Hsla,
    pub subtext: Hsla,
    pub text: Hsla,
    pub mauve: Hsla,
    pub blue: Hsla,
    pub green: Hsla,
    pub yellow: Hsla,
    pub red: Hsla,
    pub peach: Hsla,
    pub teal: Hsla,
    pub lilac: Hsla,
    pub fg_on_accent: Hsla,
    /// `--ansi-0` to `--ansi-15`.
    pub ansi: [Hsla; 16],

    // The semantic layer, the only colours a view uses.
    pub bg_sunken: Hsla,
    pub bg_chrome: Hsla,
    pub bg_surface: Hsla,
    pub bg_selected: Hsla,
    /// What sits on a surface (a selected segment, a chip's card).
    pub bg_raised: Hsla,
    pub fill: Hsla,
    pub line: Hsla,
    pub line_strong: Hsla,
    pub fg: Hsla,
    pub fg_muted: Hsla,
    pub fg_faint: Hsla,
    /// One step under `--fg-faint`: line numbers, disabled text, the quietest mark.
    pub fg_fainter: Hsla,
    pub fg_disabled: Hsla,
    pub accent: Hsla,
    /// `--ref`, a link (Rust's keyword, so the trailing underscore).
    pub ref_: Hsla,
    pub ok: Hsla,
    /// What needs the person.
    pub attention: Hsla,
    pub danger: Hsla,
    /// A file or line changed in the working tree; `attention`'s yellow means
    /// "needs you", so a change gets its own orange.
    pub changed: Hsla,
    pub info: Hsla,
    pub ring: Hsla,
    pub scrim: Hsla,
    /// `--ring-shadow`'s colour: the halo around a focused control.
    pub ring_halo: Hsla,
    /// `--mark-sel-edge`: the colour a selected row's mark is drawn in.
    pub mark_sel_edge: Hsla,

    // Derived by the page's component rules rather than named by a variable.
    /// `.btn:hover`: the text colour at 7% over whatever is behind, so a hover
    /// reads on every surface the way the page's does.
    pub hover_fill: Hsla,
    /// `.btn:active`: 12%.
    pub press_fill: Hsla,
    /// `.rfile.on:hover`: a lit row under the pointer, the selected colour with
    /// 6% of the text colour over it.
    pub selected_hover: Hsla,
    /// The text selection, and the colour a drop target is washed with.
    pub selection: Hsla,
    pub drop_target: Hsla,
    /// What the kit dims the window with behind a modal (the page's `--scrim`
    /// is another value, for its own dialogs).
    pub overlay_dim: Hsla,
    /// `.winbtn.shut:hover`: Windows' own close red and white, which no theme
    /// recolours (the stylesheet writes them as literals).
    pub win_close_bg: Hsla,
    pub win_close_fg: Hsla,

    pub scale: Scale,
}

impl Deref for Tokens {
    type Target = Scale;

    fn deref(&self) -> &Scale {
        &self.scale
    }
}

impl Global for Tokens {}

impl Tokens {
    /// The active theme's tokens.
    pub fn global(cx: &App) -> &Self {
        cx.global::<Self>()
    }
}
