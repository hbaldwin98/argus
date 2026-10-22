//! What a colour is in a theme's data, and the two mixes the page's `color-mix` formulas are made of.
//!
//! The page writes every colour as a CSS value in sRGB, so a theme here is
//! `#rrggbb` or `rgba()` and nothing else; [`mix`] and [`wash`] are
//! `color-mix(in srgb, ...)` with and without `transparent`, so a derived role
//! is the page's formula ported once and not re-derived per view.

use gpui_kit::{Hsla, Rgba};

/// A theme's colour as the stylesheet spells it: `#rrggbb`, or `rgba(r, g, b, a)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Colour {
    pub rgb: u32,
    pub alpha: f32,
}

impl Colour {
    /// `#rrggbb`.
    pub const fn hex(rgb: u32) -> Self {
        Self { rgb, alpha: 1.0 }
    }

    /// `rgba(r, g, b, a)`.
    pub const fn rgba(r: u8, g: u8, b: u8, alpha: f32) -> Self {
        Self { rgb: (r as u32) << 16 | (g as u32) << 8 | b as u32, alpha }
    }

    pub fn hsla(self) -> Hsla {
        Rgba {
            r: ((self.rgb >> 16) & 0xff) as f32 / 255.0,
            g: ((self.rgb >> 8) & 0xff) as f32 / 255.0,
            b: (self.rgb & 0xff) as f32 / 255.0,
            a: self.alpha,
        }
        .into()
    }
}

/// `color-mix(in srgb, a <share>, b)`: `share` of `a` (0 to 1) and the rest
/// `b`, interpolated premultiplied as CSS does so a see-through colour mixes
/// by what it covers.
pub fn mix(a: Hsla, share: f32, b: Hsla) -> Hsla {
    let (a, b) = (a.to_rgb(), b.to_rgb());
    let (wa, wb) = (a.a * share, b.a * (1.0 - share));
    let alpha = wa + wb;
    if alpha <= 0.0 {
        return Hsla::transparent_black();
    }
    Rgba {
        r: (a.r * wa + b.r * wb) / alpha,
        g: (a.g * wa + b.g * wb) / alpha,
        b: (a.b * wa + b.b * wb) / alpha,
        a: alpha,
    }
    .into()
}

/// `color-mix(in srgb, c <share>, transparent)`: `c` at `share` of its alpha.
pub fn wash(c: Hsla, share: f32) -> Hsla {
    c.opacity(share)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_reads_the_background_literal() {
        let rgba = Colour::hex(0x08090a).hsla().to_rgb();
        assert!((rgba.r - 8.0 / 255.0).abs() < 1e-3);
        assert!((rgba.b - 10.0 / 255.0).abs() < 1e-3);
        assert_eq!(rgba.a, 1.0);
    }

    #[test]
    fn rgba_keeps_its_alpha() {
        assert_eq!(Colour::rgba(152, 192, 239, 0.12), Colour { rgb: 0x98c0ef, alpha: 0.12 });
    }

    #[test]
    fn mixing_opaque_colours_is_the_weighted_average_per_channel() {
        let out = mix(Colour::hex(0x141516).hsla(), 0.55, Colour::hex(0x23252a).hsla()).to_rgb();
        assert!((out.r * 255.0 - 26.75).abs() < 1e-2);
        assert!((out.g * 255.0 - 28.2).abs() < 1e-2);
        assert!((out.b * 255.0 - 31.0).abs() < 1e-2);
    }

    #[test]
    fn washing_scales_alpha_and_keeps_the_colour() {
        let c = Colour::hex(0xf7f8f8).hsla();
        let w = wash(c, 0.07);
        assert!((w.a - 0.07).abs() < 1e-6);
        assert_eq!((w.h, w.s, w.l), (c.h, c.s, c.l));
    }

    #[test]
    fn mixing_with_transparent_equals_washing() {
        let c = Colour::hex(0x828fff).hsla();
        let (m, w) = (mix(c, 0.3, Hsla::transparent_black()).to_rgb(), wash(c, 0.3).to_rgb());
        assert!((m.a - w.a).abs() < 1e-6 && (m.r - w.r).abs() < 1e-5 && (m.b - w.b).abs() < 1e-5);
    }
}
