//! Which colour a packed wire colour is in the terminal's palette, and which
//! text colour stays readable on a background a program chose.
//!
//! The sixteen ANSI slots are the theme's: `page`'s `--ansi-N` tokens, read
//! from the Ledger block of `crates/argus-desktop/assets/style.css`, so a
//! program's red is the same red in both windows. The 256-colour cube and the
//! greys are not themed. The contrast floor is `panes/contrast.ts` again.

use gpui_kit::{rgb, Hsla};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub const fn hex(hex: u32) -> Self {
        Rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
    }

    pub fn hsla(self) -> Hsla {
        rgb(u32::from(self.0) << 16 | u32::from(self.1) << 8 | u32::from(self.2)).into()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Palette {
    /// The default text colour, and the cursor's.
    pub fg: Rgb,
    /// The sunken colour behind the screen, which a reversed default
    /// background and a block cursor's glyph are drawn in.
    pub bg: Rgb,
    pub ansi: [Rgb; 16],
}

impl Palette {
    /// `--fg`, `--bg-sunken` and `--ansi-0..15` of the Ledger theme.
    pub fn ledger() -> Self {
        let c = Rgb::hex;
        Self {
            fg: c(0xf7f8f8),
            bg: c(0x08090a),
            ansi: [
                c(0x23252a),
                c(0xeb5757),
                c(0x4cb782),
                c(0xf2c94c),
                c(0x4ea7fc),
                c(0x828fff),
                c(0x26b5ce),
                c(0xd0d6e0),
                c(0x62666d),
                c(0xf28b8b),
                c(0x7fd1a6),
                c(0xf7d774),
                c(0x7ab8ff),
                c(0xa3acff),
                c(0x5fd0e3),
                c(0xf7f8f8),
            ],
        }
    }

    /// A packed wire colour: `0` the default (`None`, which the caller
    /// fills in), `1 << 24 | index`, `2 << 24 | rgb`.
    pub fn resolve(&self, packed: u32) -> Option<Rgb> {
        match packed >> 24 {
            0 => None,
            2 => Some(Rgb((packed >> 16) as u8, (packed >> 8) as u8, packed as u8)),
            _ => {
                let i = packed & 255;
                Some(if i < 16 {
                    self.ansi[i as usize]
                } else if i < 232 {
                    let n = i - 16;
                    Rgb(cube_step(n / 36), cube_step(n / 6 % 6), cube_step(n % 6))
                } else {
                    let grey = (8 + 10 * (i - 232)) as u8;
                    Rgb(grey, grey, grey)
                })
            }
        }
    }
}

/// One level of the 256-colour cube.
fn cube_step(c: u32) -> u8 {
    if c == 0 {
        0
    } else {
        (55 + 40 * c) as u8
    }
}

fn luminance(c: Rgb) -> f32 {
    let linear = |v: u8| {
        let s = f32::from(v) / 255.0;
        if s <= 0.03928 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(c.0) + 0.7152 * linear(c.1) + 0.0722 * linear(c.2)
}

pub fn contrast(a: Rgb, b: Rgb) -> f32 {
    let (x, y) = (luminance(a), luminance(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

/// WCAG's floor for body text. A program that paints a red or green
/// background picks its text for its own palette, and the theme's text can
/// be as dark as that background.
pub const MINIMUM_CONTRAST: f32 = 4.5;

/// `fore` on `back`, moved toward black or white, whichever reads better on
/// `back`, in tenths of the way until it reaches the floor.
pub fn readable(fore: Rgb, back: Rgb) -> Rgb {
    if contrast(fore, back) >= MINIMUM_CONTRAST {
        return fore;
    }
    let target = if contrast(Rgb(0, 0, 0), back) >= contrast(Rgb(255, 255, 255), back) {
        Rgb(0, 0, 0)
    } else {
        Rgb(255, 255, 255)
    };
    let mut out = fore;
    for step in 1..=10 {
        let t = step as f32 / 10.0;
        let mix = |from: u8, to: u8| (f32::from(from) + (f32::from(to) - f32::from(from)) * t).round() as u8;
        out = Rgb(mix(fore.0, target.0), mix(fore.1, target.1), mix(fore.2, target.2));
        if contrast(out, back) >= MINIMUM_CONTRAST {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn ansi(i: u32) -> u32 {
        1 << 24 | i
    }

    #[test]
    fn packed_colours_resolve_as_the_page_resolves_them() {
        let palette = Palette::ledger();
        assert_eq!(palette.resolve(0), None);
        assert_eq!(palette.resolve(ansi(1)), Some(Rgb::hex(0xeb5757)));
        assert_eq!(palette.resolve(2 << 24 | 0x010203), Some(Rgb(1, 2, 3)));
        assert_eq!(palette.resolve(ansi(16)), Some(Rgb(0, 0, 0)));
        assert_eq!(palette.resolve(ansi(231)), Some(Rgb(255, 255, 255)));
        assert_eq!(palette.resolve(ansi(232)), Some(Rgb(8, 8, 8)));
        assert_eq!(palette.resolve(ansi(255)), Some(Rgb(238, 238, 238)));
    }

    #[test]
    fn the_ledger_slots_are_the_stylesheets() {
        let css = include_str!("../../../argus-desktop/assets/style.css");
        let block = css.split(":root[data-theme='ledger'] {").nth(1).expect("the Ledger ansi block");
        let block = &block[..block.find('}').unwrap()];
        let palette = Palette::ledger();
        for (slot, ours) in palette.ansi.iter().enumerate() {
            let line = block.lines().find_map(|line| line.trim().strip_prefix(&format!("--ansi-{slot}: #")));
            let hex = u32::from_str_radix(line.expect("every slot is set").trim_end_matches(';'), 16).unwrap();
            assert_eq!(*ours, Rgb::hex(hex), "--ansi-{slot}");
        }
        // The terminal's default text and ground are the theme's --text and --crust.
        let theme = css.split(".themecard [data-theme='ledger'] {").nth(1).expect("the Ledger theme block");
        let theme = &theme[..theme.find('}').unwrap()];
        let token = |name: &str| {
            let line = theme.lines().find_map(|line| line.trim().strip_prefix(&format!("--{name}: #")));
            Rgb::hex(u32::from_str_radix(line.expect("the token is set").trim_end_matches(';'), 16).unwrap())
        };
        assert_eq!((palette.fg, palette.bg), (token("text"), token("crust")));
    }

    #[test]
    fn text_the_background_would_swallow_is_moved_to_the_floor() {
        let palette = Palette::ledger();
        let (fore, back) = (palette.ansi[0], palette.ansi[8]);
        let moved = readable(fore, back);
        assert!(contrast(moved, back) >= MINIMUM_CONTRAST);
        assert_ne!(moved, fore);
    }

    #[test]
    fn readable_text_is_left_alone() {
        let palette = Palette::ledger();
        assert_eq!(readable(palette.ansi[15], palette.ansi[0]), palette.ansi[15]);
    }
}
