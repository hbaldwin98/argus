//! The page's design tokens under Ledger as plain numbers: type, spacing, radii, sizes, shadows, focus and motion.
//!
//! These are `:root` and `:root[data-theme='ledger']` in
//! `argus-desktop/assets/style.css`, copied value for value so the two windows
//! share a scale; a number here that the stylesheet does not have is a value
//! nobody chose. Colours are not here: they are [`crate::theme::Ledger`].

use std::time::Duration;

use gpui_kit::{hsla, point, px, BoxShadow, FontWeight, Pixels};

/// One of the page's `--type-*` shorthands: weight, size and line height.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Type {
    pub weight: f32,
    pub size: f32,
    /// A multiple of `size`, as the page's unitless `line-height`.
    pub leading: f32,
}

impl Type {
    pub fn weight(&self) -> FontWeight {
        FontWeight(self.weight)
    }

    pub fn size(&self) -> Pixels {
        px(self.size)
    }

    pub fn line_height(&self) -> Pixels {
        px(self.size * self.leading)
    }
}

/// `--fs-*`, the only font sizes the page draws.
pub mod fs {
    pub const XS2: f32 = 10.0;
    pub const XS: f32 = 11.0;
    pub const SM: f32 = 12.0;
    pub const MD: f32 = 13.0;
    pub const LG: f32 = 15.0;
    pub const XL: f32 = 17.0;
    pub const XL2: f32 = 20.0;
}

/// `--weight-*`.
pub mod weight {
    pub const REGULAR: f32 = 400.0;
    pub const MEDIUM: f32 = 500.0;
    pub const SEMIBOLD: f32 = 600.0;
    pub const BOLD: f32 = 700.0;
}

/// `--type-*` as Ledger sets them: its headings and labels sit between the
/// stock weights (590, 510), which the variable Inter draws as itself.
pub mod ty {
    use super::Type;

    pub const DISPLAY: Type = Type { weight: 590.0, size: 20.0, leading: 1.3 };
    pub const HERO: Type = Type { weight: 700.0, size: 28.0, leading: 1.2 };
    pub const TITLE: Type = Type { weight: 590.0, size: 17.0, leading: 1.3 };
    pub const HEADING: Type = Type { weight: 510.0, size: 15.0, leading: 1.35 };
    pub const BODY: Type = Type { weight: 400.0, size: 13.0, leading: 1.45 };
    pub const SMALL: Type = Type { weight: 400.0, size: 12.0, leading: 1.45 };
    pub const LABEL: Type = Type { weight: 510.0, size: 11.0, leading: 1.4 };
    /// `400 13px/20px`: the one with an absolute line height.
    pub const MONO: Type = Type { weight: 400.0, size: 13.0, leading: 20.0 / 13.0 };
}

/// `--sp-*`: the spacing steps, named by their size.
pub mod sp {
    pub const S2: f32 = 2.0;
    pub const S4: f32 = 4.0;
    pub const S6: f32 = 6.0;
    pub const S8: f32 = 8.0;
    pub const S12: f32 = 12.0;
    pub const S16: f32 = 16.0;
    pub const S20: f32 = 20.0;
    pub const S24: f32 = 24.0;
    pub const S32: f32 = 32.0;
    pub const S44: f32 = 44.0;
}

/// `--r-*` as Ledger sets them (`--r-md` is 8 here, 10 in the base set).
pub mod radius {
    pub const XS: f32 = 4.0;
    pub const SM: f32 = 6.0;
    pub const MD: f32 = 8.0;
    pub const LG: f32 = 12.0;
    /// `--r-pill`: the page's buttons and chips are pills.
    pub const PILL: f32 = 999.0;
}

/// Control, row and icon sizes: `--control-h*`, `--row-h*`, `--chip-h`,
/// `--icon-*`, the bars and the dots.
pub mod size {
    pub const CONTROL_H: f32 = 28.0;
    pub const CONTROL_H_SM: f32 = 24.0;
    pub const ROW_H: f32 = 28.0;
    pub const ROW_H_SM: f32 = 26.0;
    pub const CHIP_H: f32 = 20.0;
    pub const ICON_BOX: f32 = 20.0;
    pub const ICON_SM: f32 = 14.0;
    pub const ICON_LG: f32 = 24.0;
    pub const ICON_BTN: f32 = 28.0;
    pub const CHECK_BOX: f32 = 16.0;
    pub const DOT_SM: f32 = 6.0;
    pub const DOT_MD: f32 = 7.0;
    pub const DOT_LG: f32 = 8.0;
    pub const METER_H: f32 = 6.0;
    pub const HEADER_H: f32 = 56.0;
    pub const STRIP_H: f32 = 40.0;
    pub const PANE_HEAD_H: f32 = 32.0;
    pub const RAIL_W: f32 = 288.0;
    pub const RAIL_FOLDED: f32 = 56.0;
    pub const RAIL_TILE: f32 = 40.0;
    /// `.sdot`: the box a status dot, ring or dash sits in, and the dash's height.
    pub const STATUS_BOX: f32 = 9.0;
    pub const STATUS_DASH_H: f32 = 1.5;
    /// A nav row's icon cell, the indent of a checkout's panes under its branch, and the open pane's edge mark.
    pub const NAV_ICON_CELL: f32 = 22.0;
    pub const PANE_INDENT: f32 = 38.0;
    pub const PANE_MARK_W: f32 = 2.0;
    /// A window control (minimise, maximise, close) is this wide.
    pub const WINDOW_CONTROL_W: f32 = 46.0;
    /// `.kbd`'s line height and `.fbtn`'s height.
    pub const KEY_LEADING: f32 = 14.0;
    pub const FOLD_BTN_H: f32 = 22.0;
}

/// `--shadow-pop`: a menu or a popover under Ledger.
pub fn shadow_pop() -> Vec<BoxShadow> {
    vec![drop_shadow(8.0, 24.0, 0.5)]
}

/// `--shadow-dialog`: a dialog (the base value; Ledger does not override it).
pub fn shadow_dialog() -> Vec<BoxShadow> {
    vec![drop_shadow(24.0, 64.0, 0.45)]
}

fn drop_shadow(dy: f32, blur: f32, alpha: f32) -> BoxShadow {
    BoxShadow {
        color: hsla(0.0, 0.0, 0.0, alpha),
        offset: point(px(0.0), px(dy)),
        blur_radius: px(blur),
        spread_radius: px(0.0),
        inset: false,
    }
}

/// `--opacity-dim`: what a dimmed control keeps.
pub const OPACITY_DIM: f32 = 0.55;

/// The focus ring: a `:focus-visible` outline of this width in `--ring`, and
/// the halo (`--ring-shadow`) of this width at this share of the ring colour.
pub mod focus {
    pub const OUTLINE: f32 = 2.0;
    pub const HALO: f32 = 3.0;
    pub const HALO_ALPHA: f32 = 0.30;
}

/// The page's few durations: `--t-fast` on hovers and fades, the chevron's
/// 0.15 s turn, a spinner's turn, the changed-file flash and the placeholder
/// shimmer. There are no custom easings: transitions use `ease`, the flash
/// `ease-out`, the shimmer `ease-in-out` and spinners `linear`.
pub mod motion {
    use super::Duration;

    pub const FAST: Duration = Duration::from_millis(120);
    pub const TURN: Duration = Duration::from_millis(150);
    pub const SPIN: Duration = Duration::from_millis(900);
    pub const SPIN_SLOW: Duration = Duration::from_millis(1000);
    pub const SHIMMER: Duration = Duration::from_millis(1400);
    pub const FLASH: Duration = Duration::from_millis(2400);
}

#[cfg(test)]
mod tests {
    use super::*;

    const CSS: &str = include_str!("../../argus-desktop/assets/style.css");

    /// A `--name: Npx;` declaration, in `from` if given and only there.
    fn px_of(from: &str, name: &str) -> f32 {
        let at = from.find(&format!("  --{name}: ")).unwrap_or_else(|| panic!("--{name} is not in the stylesheet"));
        let rest = &from[at + name.len() + 6..];
        let end = rest.find("px;").unwrap_or_else(|| panic!("--{name} is not in px"));
        rest[..end].parse().unwrap_or_else(|_| panic!("--{name} is not a number"))
    }

    /// The `:root[data-theme='ledger']` block's shape (it opens with Inter), where Ledger overrides the base set.
    fn ledger() -> &'static str {
        let from = CSS.find("--font-ui-bundled: 'Inter', 'Geist'").expect("the ledger block");
        let to = from + CSS[from..].find("\n}").expect("the block's end");
        &CSS[from..to]
    }

    // The scale is read from `style.css` (`:root`, then the ledger block of
    // "Shape per theme"), so a drift from the page fails here.
    #[test]
    fn the_scale_is_the_stylesheets() {
        let fs_of = |name: &str| px_of(CSS, name);
        assert_eq!(
            [fs::XS2, fs::XS, fs::SM, fs::MD, fs::LG, fs::XL, fs::XL2],
            ["fs-2xs", "fs-xs", "fs-sm", "fs-md", "fs-lg", "fs-xl", "fs-2xl"].map(fs_of)
        );
        assert_eq!(
            [sp::S2, sp::S4, sp::S6, sp::S8, sp::S12, sp::S16, sp::S20, sp::S24, sp::S32, sp::S44],
            ["sp-2", "sp-4", "sp-6", "sp-8", "sp-12", "sp-16", "sp-20", "sp-24", "sp-32", "sp-44"].map(fs_of)
        );
        let ledger = ledger();
        assert_eq!(
            [radius::XS, radius::SM, radius::MD, radius::LG],
            ["r-xs", "r-sm", "r-md", "r-lg"].map(|name| px_of(ledger, name))
        );
        assert_eq!(size::CONTROL_H, px_of(ledger, "control-h"));
        assert_eq!(size::CONTROL_H_SM, px_of(ledger, "control-h-sm"));
        assert_eq!(size::ICON_BTN, px_of(ledger, "icon-btn"));
        assert_eq!([size::ROW_H, size::ROW_H_SM], [px_of(CSS, "row-h"), px_of(CSS, "row-h-sm")]);
        assert_eq!([size::RAIL_FOLDED, size::RAIL_TILE], [px_of(CSS, "rail-folded"), px_of(CSS, "rail-tile")]);
        // Type pairs weight, size and leading, which the stylesheet writes as one shorthand.
        assert_eq!(ty::BODY, Type { weight: 400.0, size: 13.0, leading: 1.45 });
        assert_eq!(ty::HEADING.weight, 510.0);
        assert_eq!(ty::TITLE.weight, 590.0);
        assert_eq!(ty::MONO.line_height(), px(20.0));
        assert_eq!(motion::FAST, Duration::from_millis(120));
    }

    #[test]
    fn the_popover_shadow_is_ledgers() {
        let pop = shadow_pop();
        assert_eq!(pop.len(), 1);
        assert_eq!(pop[0].blur_radius, px(24.0));
        assert_eq!(pop[0].offset.y, px(8.0));
        assert!((pop[0].color.a - 0.5).abs() < 1e-6);
    }
}
