//! Holds the token space to the stylesheet: Ledger's tokens read back from `style.css`, so a drift from the page fails here.
//!
//! The stylesheet is the source of every value; this module only reads it. The
//! palette and `--ansi-N` come from Ledger's blocks, the scale from `:root`
//! with Ledger's "Shape per theme" block over it, and the derived roles from
//! the `color-mix` formulas the page writes, whose shares are parsed out of
//! the rules that use them.

use std::time::Duration;

use super::colour::Colour;
use super::ledger;
use super::scale::{Layer, Mark, Type};

const CSS: &str = include_str!("../../../argus-desktop/assets/style.css");

/// The text of each `:root[data-theme='ledger']` block, in file order: the
/// palette, the sixteen ANSI colours and the shape.
fn ledger_blocks() -> Vec<&'static str> {
    CSS.match_indices(":root[data-theme='ledger']")
        .map(|(from, _)| {
            let open = from + CSS[from..].find('{').expect("the block opens");
            let close = open + CSS[open..].find("\n}").expect("the block closes");
            &CSS[open..close]
        })
        .collect()
}

/// The `:root` block holding the scale (it opens with the bundled interface face).
fn root_block() -> &'static str {
    let from = CSS.find("\n:root {\n  --font-ui-bundled").expect("the scale's :root block");
    &CSS[from..from + CSS[from..].find("\n}").expect("the block's end")]
}

/// A `--name: value;` declaration in `block`; the value may span lines.
fn decl(block: &str, name: &str) -> Option<String> {
    let at = block.find(&format!("\n  --{name}:"))? + name.len() + 6;
    let rest = &block[at..];
    let value = &rest[..rest.find(';')?];
    Some(value.split_whitespace().collect::<Vec<_>>().join(" "))
}

/// Ledger's value for `name`: its own block if it sets one, else `:root`'s.
fn value(name: &str) -> String {
    let blocks = ledger_blocks();
    decl(blocks[2], name).or_else(|| decl(root_block(), name)).unwrap_or_else(|| panic!("--{name} is in neither block"))
}

fn px_of(name: &str) -> f32 {
    let v = value(name);
    v.strip_suffix("px").unwrap_or(&v).parse().unwrap_or_else(|_| panic!("--{name} is `{v}`, not a length"))
}

fn colour_of(block: &str, name: &str) -> Colour {
    let v = decl(block, name).unwrap_or_else(|| panic!("--{name} is not set"));
    Colour::hex(u32::from_str_radix(v.trim_start_matches('#'), 16).unwrap_or_else(|_| panic!("--{name} is `{v}`")))
}

/// `weight size/leading` of a `--type-*` shorthand; a leading in px is a ratio of the size.
fn type_of(name: &str) -> Type {
    let v = value(name);
    let mut words = v.split(' ');
    let weight: f32 = words.next().unwrap().parse().unwrap();
    let (size, leading) = words.next().unwrap().split_once('/').unwrap();
    let size: f32 = size.trim_end_matches("px").parse().unwrap();
    let leading = match leading.strip_suffix("px") {
        Some(px) => px.parse::<f32>().unwrap() / size,
        None => leading.parse().unwrap(),
    };
    Type::new(weight, size, leading)
}

/// The layers of a `--shadow-*`: `<dx>px <dy>px <blur>px [<spread>px] rgba(r, g, b, a)`, comma-separated.
fn layers_of(name: &str) -> Vec<Layer> {
    let v = value(name);
    // Split on the commas outside parentheses.
    let (mut parts, mut depth, mut from) = (Vec::new(), 0, 0);
    for (at, ch) in v.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(&v[from..at]);
                from = at + 1;
            }
            _ => {}
        }
    }
    parts.push(&v[from..]);
    parts
        .into_iter()
        .map(|part| {
            let part = part.trim();
            let at = part.find("rgba(").unwrap_or_else(|| panic!("--{name}: no rgba in `{part}`"));
            let lengths: Vec<f32> = part[..at].split_whitespace().map(|n| n.trim_end_matches("px").parse().unwrap()).collect();
            let channels: Vec<f32> = part[at + 5..].trim_end_matches(')').split(',').map(|n| n.trim().parse().unwrap()).collect();
            Layer {
                dx: lengths[0],
                dy: lengths[1],
                blur: lengths[2],
                spread: lengths.get(3).copied().unwrap_or(0.0),
                colour: Colour::rgba(channels[0] as u8, channels[1] as u8, channels[2] as u8, channels[3]),
            }
        })
        .collect()
}

/// A hsla colour as the 8-bit `[r, g, b]` a screen shows.
fn bytes(c: gpui_kit::Hsla) -> [u8; 3] {
    let c = c.to_rgb();
    [c.r, c.g, c.b].map(|v| (v * 255.0).round() as u8)
}

fn hex_bytes(c: Colour) -> [u8; 3] {
    [(c.rgb >> 16) as u8, (c.rgb >> 8) as u8, c.rgb as u8]
}

/// The share `rule` gives its first operand: `color-mix(in srgb, A 55%, B)` is 0.55.
fn share_in(rule: &str) -> f32 {
    let at = rule.find('%').expect("a share");
    let start = rule[..at].rfind(|c: char| !c.is_ascii_digit()).unwrap() + 1;
    rule[start..at].parse::<f32>().unwrap() / 100.0
}

fn sheet_has(rule: &str) -> &str {
    assert!(CSS.contains(rule), "the stylesheet no longer says `{rule}`");
    rule
}

#[test]
fn ledgers_palette_is_the_stylesheets() {
    let t = ledger::tokens();
    let block = ledger_blocks()[0];
    for (name, ours) in [
        ("crust", t.crust),
        ("mantle", t.mantle),
        ("base", t.base),
        ("surface0", t.surface0),
        ("surface1", t.surface1),
        ("overlay", t.overlay),
        ("subtext", t.subtext),
        ("text", t.text),
        ("mauve", t.mauve),
        ("blue", t.blue),
        ("green", t.green),
        ("yellow", t.yellow),
        ("red", t.red),
        ("peach", t.peach),
        ("teal", t.teal),
        ("lilac", t.lilac),
        ("fg-on-accent", t.fg_on_accent),
    ] {
        assert_eq!(bytes(ours), hex_bytes(colour_of(block, name)), "--{name}");
    }
}

#[test]
fn ledgers_ansi_sixteen_are_the_stylesheets() {
    let t = ledger::tokens();
    let block = ledger_blocks()[1];
    for (slot, ours) in t.ansi.into_iter().enumerate() {
        assert_eq!(bytes(ours), hex_bytes(colour_of(block, &format!("ansi-{slot}"))), "--ansi-{slot}");
    }
}

#[test]
fn the_semantic_roles_are_the_palette_the_stylesheet_aliases_them_to() {
    // `:root` aliases each role to a palette name (`--bg-chrome: var(--mantle)`), which every
    // theme without its own line for the role inherits.
    let t = ledger::tokens();
    let mocha = CSS.split("\n}").next().unwrap();
    for (role, palette, ours) in [
        ("bg-sunken", "crust", t.bg_sunken),
        ("bg-chrome", "mantle", t.bg_chrome),
        ("bg-surface", "base", t.bg_surface),
        ("bg-selected", "lilac", t.bg_selected),
        ("fill", "surface0", t.fill),
        ("line", "surface0", t.line),
        ("line-strong", "surface1", t.line_strong),
        ("fg", "text", t.fg),
        ("fg-muted", "subtext", t.fg_muted),
        ("fg-faint", "overlay", t.fg_faint),
        ("fg-disabled", "surface1", t.fg_disabled),
        ("accent", "mauve", t.accent),
        ("ref", "blue", t.ref_),
        ("ok", "green", t.ok),
        ("attention", "yellow", t.attention),
        ("danger", "red", t.danger),
        ("changed", "peach", t.changed),
        ("info", "teal", t.info),
    ] {
        assert_eq!(decl(mocha, role).as_deref(), Some(format!("var(--{palette})").as_str()), "--{role}");
        assert_eq!(bytes(ours), hex_bytes(colour_of(ledger_blocks()[0], palette)), "--{role}");
    }
}

#[test]
fn the_derived_roles_follow_the_pages_formulas() {
    let t = ledger::tokens();
    // `--bg-raised: color-mix(in srgb, var(--bg-surface) 55%, var(--fill))`, per 8-bit channel.
    let rule = decl(root_block(), "bg-raised").unwrap();
    assert_eq!(rule, "color-mix(in srgb, var(--bg-surface) 55%, var(--fill))");
    let (surface, fill) = (hex_bytes(colour_of(ledger_blocks()[0], "base")), hex_bytes(colour_of(ledger_blocks()[0], "surface0")));
    let want = [0, 1, 2].map(|i| (share_in(&rule) * f32::from(surface[i]) + (1.0 - share_in(&rule)) * f32::from(fill[i])).round() as u8);
    assert_eq!(bytes(t.bg_raised), want);
    assert_eq!(want, [0x1b, 0x1c, 0x1f]);

    // `--ring: var(--accent)` and `--ring-shadow` washes it at the stylesheet's share.
    assert_eq!(decl(root_block(), "ring").as_deref(), Some("var(--accent)"));
    assert_eq!(t.ring, t.accent);
    let halo = decl(root_block(), "ring-shadow").unwrap();
    assert!((t.ring_halo.a - share_in(&halo)).abs() < 1e-6);
    assert_eq!(t.ring_halo_width, halo.split(' ').nth(3).unwrap().trim_end_matches("px").parse::<f32>().unwrap());

    // The hover and press fills and the lit row's hover are rules of their own.
    assert!((t.hover_fill.a - share_in(sheet_has("color-mix(in srgb, var(--fg) 7%, transparent)"))).abs() < 1e-6);
    assert!((t.press_fill.a - share_in(sheet_has("color-mix(in srgb, var(--fg) 12%, transparent)"))).abs() < 1e-6);
    let lit = sheet_has("color-mix(in srgb, var(--bg-selected), var(--fg) 6%)");
    let (selected, fg) = (hex_bytes(colour_of(ledger_blocks()[0], "lilac")), hex_bytes(colour_of(ledger_blocks()[0], "text")));
    let share = share_in(lit);
    let want = [0, 1, 2].map(|i| ((1.0 - share) * f32::from(selected[i]) + share * f32::from(fg[i])).round() as u8);
    assert_eq!(bytes(t.selected_hover), want);

    // Windows' close red is a literal in `.winbtn.shut:hover`.
    sheet_has(".winbtn.shut:hover {\n  background: #c42b1c;\n  color: #ffffff;\n}");
    assert_eq!(bytes(t.win_close_bg), [0xc4, 0x2b, 0x1c]);
    assert_eq!(bytes(t.win_close_fg), [0xff, 0xff, 0xff]);

    // Ledger's selection is a hairline in `--line-strong`.
    assert_eq!(value("mark-sel"), "inset 0 0 0 1px var(--line-strong)");
    assert_eq!(t.mark_sel, Mark::Hairline);
    assert_eq!(t.mark_sel_edge, t.line_strong);
    assert_eq!(value("scrim"), "rgba(17, 17, 27, 0.62)");
    assert_eq!(bytes(t.scrim), [17, 17, 27]);
}

#[test]
fn ledgers_scale_is_the_stylesheets() {
    let t = ledger::tokens();
    for (name, ours) in [
        ("fs-2xs", t.fs_2xs),
        ("fs-xs", t.fs_xs),
        ("fs-sm", t.fs_sm),
        ("fs-md", t.fs_md),
        ("fs-lg", t.fs_lg),
        ("fs-xl", t.fs_xl),
        ("fs-2xl", t.fs_2xl),
        ("sp-2", t.sp_2),
        ("sp-4", t.sp_4),
        ("sp-6", t.sp_6),
        ("sp-8", t.sp_8),
        ("sp-12", t.sp_12),
        ("sp-16", t.sp_16),
        ("sp-20", t.sp_20),
        ("sp-24", t.sp_24),
        ("sp-32", t.sp_32),
        ("sp-44", t.sp_44),
        ("r-xs", t.r_xs),
        ("r-sm", t.r_sm),
        ("r-md", t.r_md),
        ("r-lg", t.r_lg),
        ("r-pill", t.r_pill),
        ("control-h", t.control_h),
        ("control-h-sm", t.control_h_sm),
        ("row-h", t.row_h),
        ("row-h-sm", t.row_h_sm),
        ("chip-h", t.chip_h),
        ("icon-box", t.icon_box),
        ("icon-sm", t.icon_sm),
        ("icon-lg", t.icon_lg),
        ("icon-btn", t.icon_btn),
        ("check-box", t.check_box),
        ("dot-sm", t.dot_sm),
        ("dot-md", t.dot_md),
        ("dot-lg", t.dot_lg),
        ("meter-h", t.meter_h),
        ("header-h", t.header_h),
        ("strip-h", t.strip_h),
        ("pane-head-h", t.pane_head_h),
        ("rail-folded", t.rail_folded),
        ("rail-tile", t.rail_tile),
    ] {
        assert_eq!(ours, px_of(name), "--{name}");
    }
    for (name, ours) in [
        ("weight-regular", t.weight_regular),
        ("weight-medium", t.weight_medium),
        ("weight-semibold", t.weight_semibold),
        ("weight-bold", t.weight_bold),
        ("opacity-dim", t.opacity_dim),
    ] {
        assert_eq!(ours, value(name).parse::<f32>().unwrap(), "--{name}");
    }
    assert_eq!(value("rail-w"), "var(--rail-set, 288px)");
    assert_eq!(t.rail_w, 288.0);
    assert_eq!(t.t_fast, Duration::from_millis(value("t-fast").trim_end_matches("ms").parse().unwrap()));
    for (name, ours) in [
        ("type-display", t.type_display),
        ("type-hero", t.type_hero),
        ("type-title", t.type_title),
        ("type-heading", t.type_heading),
        ("type-body", t.type_body),
        ("type-small", t.type_small),
        ("type-label", t.type_label),
        ("type-mono", t.type_mono),
    ] {
        let want = type_of(name);
        assert_eq!((ours.weight, ours.size), (want.weight, want.size), "--{name}");
        assert!((ours.leading - want.leading).abs() < 1e-6, "--{name} leading");
    }
    assert_eq!(t.shadow_pop, layers_of("shadow-pop"));
    assert_eq!(t.shadow_dialog, layers_of("shadow-dialog"));
}

#[test]
fn ledgers_faces_lead_the_stylesheets_stacks() {
    let t = ledger::tokens();
    let first = |name: &str| value(name).split(',').next().unwrap().trim().trim_matches('\'').to_owned();
    assert_eq!(t.font_ui, first("font-ui-bundled"));
    assert_eq!(t.font_mono, first("font-mono-bundled"));
}

#[test]
fn the_ports_own_sizes_are_named_not_themed() {
    // Sizes with no variable are the same in every theme, so a theme that sets none keeps them.
    let t = ledger::tokens();
    assert_eq!(t.window_control_w, 46.0);
    assert_eq!(t.status_box, 9.0);
}
