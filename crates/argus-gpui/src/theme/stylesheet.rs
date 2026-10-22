//! Holds the token space to the stylesheet: Ledger's tokens read back from `style.css`, so a drift from the page fails here.
//!
//! The stylesheet is the source of every value; this module only reads it. The
//! palette and `--ansi-N` come from Ledger's blocks, the scale from `:root`
//! with Ledger's "Shape per theme" block over it, and the derived roles from
//! the `color-mix` formulas the page writes, whose shares are parsed out of
//! the rules that use them.

use std::time::Duration;

use super::colour::Colour;
use super::themes::ledger;
use super::scale::{Layer, Mark, Type};

const CSS: &str = include_str!("../../../argus-desktop/assets/style.css");

/// The text of each `:root[data-theme='<theme>']` block, in file order: the palette, the sixteen
/// ANSI colours and (for the themes that have one) the shape.
pub(super) fn blocks(theme: &str) -> Vec<&'static str> {
    let mut opens = Vec::new();
    for (from, _) in CSS.match_indices(&format!(":root[data-theme='{theme}']")) {
        let open = from + CSS[from..].find('{').expect("the block opens");
        if !opens.contains(&open) {
            opens.push(open);
        }
    }
    opens
        .into_iter()
        .map(|open| {
            let close = open + CSS[open..].find("\n}").expect("the block closes");
            &CSS[open..close]
        })
        .collect()
}

fn ledger_blocks() -> Vec<&'static str> {
    blocks("ledger")
}

/// The `:root` block holding the scale (it opens with the bundled interface face).
pub(super) fn root_block() -> &'static str {
    let from = CSS.find("\n:root {\n  --font-ui-bundled").expect("the scale's :root block");
    &CSS[from..from + CSS[from..].find("\n}").expect("the block's end")]
}

/// A `--name: value;` declaration in `block`; the value may span lines.
pub(super) fn decl(block: &str, name: &str) -> Option<String> {
    let at = block.find(&format!("\n  --{name}:"))? + name.len() + 6;
    let rest = &block[at..];
    let value = &rest[..rest.find(';')?];
    Some(value.split_whitespace().collect::<Vec<_>>().join(" "))
}

/// `theme`'s value for `name`: the last of its blocks that sets one, else `:root`'s.
pub(super) fn value_in(theme: &str, name: &str) -> Option<String> {
    blocks(theme).into_iter().rev().find_map(|block| decl(block, name)).or_else(|| decl(root_block(), name))
}

/// Ledger's value for `name`: its own block if it sets one, else `:root`'s.
fn value(name: &str) -> String {
    value_in("ledger", name).unwrap_or_else(|| panic!("--{name} is in neither block"))
}

fn px_of(name: &str) -> f32 {
    let v = value(name);
    v.strip_suffix("px").unwrap_or(&v).parse().unwrap_or_else(|_| panic!("--{name} is `{v}`, not a length"))
}

pub(super) fn colour_of(block: &str, name: &str) -> Colour {
    let v = decl(block, name).unwrap_or_else(|| panic!("--{name} is not set"));
    Colour::hex(u32::from_str_radix(v.trim_start_matches('#'), 16).unwrap_or_else(|_| panic!("--{name} is `{v}`")))
}

/// `weight size/leading` of a `--type-*` shorthand; a leading in px is a ratio of the size.
fn type_of(name: &str) -> Type {
    type_in(&value(name))
}

pub(super) fn type_in(v: &str) -> Type {
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
    layers_in(&value(name), name)
}

pub(super) fn layers_in(v: &str, name: &str) -> Vec<Layer> {
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
            let (at, colour) = match part.find("rgba(") {
                Some(at) => {
                    let c: Vec<f32> = part[at + 5..].trim_end_matches(')').split(',').map(|n| n.trim().parse().unwrap()).collect();
                    (at, Colour::rgba(c[0] as u8, c[1] as u8, c[2] as u8, c[3]))
                }
                None => {
                    let at = part.find('#').unwrap_or_else(|| panic!("--{name}: no colour in `{part}`"));
                    (at, Colour::hex(u32::from_str_radix(&part[at + 1..], 16).unwrap()))
                }
            };
            let lengths: Vec<f32> = part[..at].split_whitespace().map(|n| n.trim_end_matches("px").parse().unwrap()).collect();
            Layer { dx: lengths[0], dy: lengths[1], blur: lengths[2], spread: lengths.get(3).copied().unwrap_or(0.0), colour }
        })
        .collect()
}

/// A hsla colour as the 8-bit `[r, g, b]` a screen shows.
pub(super) fn bytes(c: gpui_kit::Hsla) -> [u8; 3] {
    let c = c.to_rgb();
    [c.r, c.g, c.b].map(|v| (v * 255.0).round() as u8)
}

pub(super) fn hex_bytes(c: Colour) -> [u8; 3] {
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
        ("icon-glyph", t.icon_glyph),
        ("drawer-max", t.drawer_max),
        ("dock-floor", t.dock_floor),
        ("dock-min-pane", t.dock_min_pane),
        ("dock-room", t.dock_room),
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
    assert_eq!(t.dialog_w, 420.0);
}

#[test]
fn the_shared_parts_sizes_are_the_rules_they_were_written_from() {
    let t = ledger::tokens();
    sheet_has(".pbar {
  width: 56px;
  height: var(--meter-h);");
    assert_eq!(t.meter_w, 56.0);
    assert_eq!(t.meter_sm_w, 60.0, "`.vmeter-sm`");
    sheet_has(".ghost {
  height: 12px;");
    assert_eq!(t.ghost_h, 12.0);
}

#[test]
fn the_drawn_blocks_are_the_rules_they_were_written_from() {
    let t = ledger::tokens();
    sheet_has("  max-width: min(100%, 720px);
  max-height: 360px;");
    assert_eq!((t.chart_max_w, t.chart_max_h), (720.0, 360.0));
    sheet_has(".vchart-line {
  fill: none;
  stroke: var(--t);
  stroke-width: 2;");
    sheet_has(".vchart-key {
  stroke: var(--t);
  stroke-width: 2;");
    assert_eq!(t.chart_stroke, 2.0);
    sheet_has(".vspark {
  display: block;
  width: 100%;
  height: 32px;");
    sheet_has(".vchart-spark .vspark {
  max-width: 240px;");
    sheet_has(".vstat .vspark {
  margin-top: var(--sp-4);
  height: 28px;");
    assert_eq!((t.spark_h, t.spark_max_w, t.stat_spark_h), (32.0, 240.0, 28.0));
    sheet_has(".vtable-table td {
  height: 34px;");
    sheet_has("  max-width: 280px;
  overflow: hidden;
  text-overflow: ellipsis;");
    sheet_has(".vtable-filter {
  min-width: 140px;");
    assert_eq!((t.table_row_h, t.table_long_max_w, t.table_filter_min_w), (34.0, 280.0, 140.0));
    sheet_has("  font-family: var(--font-ui);
  font-size: 12px;");
    sheet_has(".vd-num {
  fill: var(--fg);
  font-size: 10px;");
    assert_eq!((t.vd_font, t.vd_num_font), (12.0, 10.0));
    sheet_has(".vd-box {
  fill: var(--bg-surface);
  stroke: var(--line-strong);
  stroke-width: 1;");
    sheet_has(".vd-actor {
  fill: none;
  stroke: var(--fg-muted);
  stroke-width: 1.5;");
    sheet_has(".vd-life {
  stroke: var(--line-strong);
  stroke-width: 1;
  stroke-dasharray: 4 4;");
    sheet_has(".vd-msg {
  fill: none;
  stroke: var(--fg-muted);
  stroke-width: 1.4;");
    sheet_has(".vd-reply {
  stroke-dasharray: 5 4;");
    assert_eq!((t.vd_stroke, t.vd_msg_stroke, t.vd_actor_stroke), (1.0, 1.4, 1.5));
    assert_eq!((t.vd_life_dash, t.vd_reply_on, t.vd_reply_off), (4.0, 5.0, 4.0));
    sheet_has(".vg-box {
  fill: var(--bg-surface);
  stroke: var(--t, var(--fg-muted));
  stroke-width: 1.5;");
    sheet_has(".vg-edge {
  fill: none;
  stroke: var(--fg-muted);
  stroke-width: 1.3;");
    sheet_has("  font-size: 11px;
  paint-order: stroke;
  stroke: var(--bg-sunken);
  stroke-width: 4px;");
    assert_eq!((t.vg_stroke, t.vg_edge_stroke, t.vg_label_font, t.vg_halo), (1.5, 1.3, 11.0, 4.0));
    sheet_has(".vg svg {
  display: block;
  width: 100%;
  height: auto;
  margin: 0 auto;
  font-family: var(--font-ui);
  font-size: 12px;");
    assert_eq!(t.vg_node_font, 12.0);
    let js = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../argus-desktop/page/src/views/blocks/diagram.ts")).unwrap();
    assert!(js.contains("markerWidth: 9,") && js.contains("rx: 5 }, 'vd-box'") && js.contains("height: 22, rx: 3 }, 'vd-branch'"), "diagram.ts no longer draws what vd_head, vd_box_radius and vd_branch_radius say");
    assert_eq!((t.vd_head, t.vd_box_radius, t.vd_branch_radius), (9.0, 5.0, 3.0));
    let js = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../argus-desktop/page/src/views/blocks/graph.ts")).unwrap();
    assert!(js.contains("markerWidth: 8,") && js.contains("rx: 6 }, 'vg-box'"), "graph.ts no longer draws what vg_head and vg_radius say");
    assert_eq!((t.vg_head, t.vg_radius), (8.0, 6.0));
}

#[test]
fn the_stage_strip_drops_the_size_label_in_a_narrow_window() {
    let t = ledger::tokens();
    sheet_has("@media (max-width: 1100px) {
  .head-size {
    display: none;");
    assert_eq!(t.head_size_hidden_below, 1100.0);
}

#[test]
fn the_window_is_narrow_at_the_1200_breakpoint_where_the_strip_says_the_status() {
    let t = ledger::tokens();
    sheet_has("@media (max-width: 1200px) {
  .head-status {
    display: inline-flex;");
    assert_eq!(t.narrow_below, 1200.0);
}

#[test]
fn the_empty_stages_blocks_and_the_stopped_card_are_the_rules_they_were_written_from() {
    let t = ledger::tokens();
    assert_eq!(t.tile, px_of("tile"));
    sheet_has("--dialog-w-sm: min(440px, 94vw);");
    assert_eq!(t.dialog_w_sm, 440.0);
    sheet_has(".empty-body {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--sp-16);
  max-width: 520px;");
    sheet_has(".empty-body.no-projects {
  max-width: 460px;");
    sheet_has(".empty-body.no-pane {
  max-width: 760px;");
    assert_eq!((t.empty_w, t.empty_w_projects, t.empty_w_pane), (520.0, 460.0, 760.0));
    assert_eq!(t.stopped_icon, 28.0, "index.html draws the stopped icon at 28");
}

#[test]
fn the_blocks_of_an_agents_view_are_the_rules_they_were_written_from() {
    let t = ledger::tokens();
    sheet_has(".vcolumns {\n  display: grid;\n  grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));");
    assert_eq!(t.col_min, 240.0);
    sheet_has(".vgrid {\n  display: grid;\n  gap: var(--sp-12);\n  grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));");
    sheet_has(".vgrid.sm {\n  grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));");
    sheet_has(".vgrid.lg {\n  grid-template-columns: repeat(auto-fill, minmax(360px, 1fr));");
    assert_eq!((t.grid_min_sm, t.grid_min_md, t.grid_min_lg), (160.0, 240.0, 360.0));
    sheet_has(".vstat-value {\n  font: 650 22px/1.2 var(--font-ui);");
    assert_eq!(t.type_stat, Type::new(650.0, 22.0, 1.2));
    sheet_has(".vtl-event {\n  display: grid;\n  grid-template-columns: 84px 12px minmax(0, 1fr);");
    sheet_has(".vtl-dot {\n  width: 12px;\n  height: 12px;");
    sheet_has("  top: 14px;\n  bottom: -2px;\n  width: 2px;\n  background: var(--line);");
    assert_eq!((t.tl_rule_top, t.tl_rule_bleed), (14.0, 2.0));
    sheet_has("@media (max-width: 640px) {\n  .vtl-lanes {");
    assert_eq!((t.tl_at_w, t.tl_dot, t.tl_rule_w, t.tl_stack_below), (84.0, 12.0, 2.0, 640.0));
    sheet_has(".vlog {\n  max-height: 320px;");
    sheet_has(".vcode-bd {\n  font-family: var(--font-mono);\n  font-size: var(--fs-sm);\n  line-height: 18px;");
    sheet_has("  margin: 0;\n  max-height: 360px;");
    assert_eq!((t.log_max_h, t.code_max_h, t.code_line_h), (320.0, 360.0, 18.0));
    sheet_has(".vmatrix {\n  border-collapse: separate;\n  border-spacing: 2px;");
    sheet_has(".vmatrix-cell {\n  min-width: 44px;\n  height: 32px;");
    assert_eq!((t.matrix_gap, t.matrix_cell_w, t.matrix_cell_h), (2.0, 44.0, 32.0));
    sheet_has(".vboard-col {\n  flex: 0 0 240px;");
    assert_eq!(t.board_col_w, 240.0);
    sheet_has(".vfields {\n  display: grid;\n  grid-template-columns: 132px minmax(0, 1fr);");
    assert_eq!(t.fields_label_w, 132.0);
    sheet_has(".vlist-item {\n  display: grid;\n  grid-template-columns: 20px minmax(0, 1fr);");
    sheet_has(".vlist-item:not(.vlist-event) .vlist-body {\n  margin-left: 28px;");
    assert_eq!((t.list_lead_w, t.list_indent), (20.0, 28.0));
    sheet_has(".vspin {\n  width: 10px;\n  height: 10px;");
    assert_eq!(t.spin_sm, 10.0);
    sheet_has("  border: 1.5px solid var(--line-strong);\n  border-radius: var(--r-xs);\n  color: var(--fg-on-accent);");
    assert_eq!(t.check_edge, 1.5);
    sheet_has(".vcard.tinted {
  border-color: color-mix(in srgb, var(--t) 55%, var(--line));");
    assert_eq!(t.share_card_edge, 0.55);
    sheet_has("  background: color-mix(in srgb, var(--t) 18%, transparent);");
    sheet_has("  background: color-mix(in srgb, var(--t) 16%, transparent);");
    sheet_has("  background: color-mix(in srgb, var(--t, var(--fg-muted)) 24%, var(--fill));");
    assert_eq!((t.share_meter_track, t.share_chip, t.share_avatar), (0.18, 0.16, 0.24));
    sheet_has(".vcallout {
  display: flex;
  gap: var(--sp-8);
  align-items: flex-start;
  border-radius: var(--r-md);
  padding: var(--sp-8) var(--sp-12);
  background: color-mix(in srgb, var(--t) 12%, transparent);");
    assert_eq!(t.share_callout, 0.12, "the callout wash is the warn callout's");
    sheet_has(".vact .btn[disabled] {
  cursor: default;
  opacity: 0.7;");
    assert_eq!(t.opacity_wait, 0.7);
}

#[test]
fn a_card_of_a_pane_to_go_back_to_is_the_rules_it_was_written_from() {
    let t = ledger::tokens();
    assert_eq!(t.card_min, px_of("vstat-min"));
    sheet_has(".rcard:hover {
  border-color: var(--line-strong);
  background: color-mix(in srgb, var(--bg-raised) 88%, var(--fg) 6%);");
    sheet_has(".rcard:active {
  background: color-mix(in srgb, var(--bg-raised) 80%, var(--fg) 10%);");
    assert_eq!((t.share_card_hover, t.share_card_press), (0.06, 0.10));
}

#[test]
fn the_forms_palettes_and_sheets_sizes_are_the_rules_they_were_written_from() {
    let t = ledger::tokens();
    assert_eq!(value("dialog-w"), "min(560px, 94vw)");
    assert_eq!(t.form_w, 560.0);
    sheet_has(".palette {\n  width: min(620px, 92vw);");
    assert_eq!(t.palette_w, 620.0);
    sheet_has(".dialog.sheet-dialog.keys-sheet {\n  width: min(960px, 94vw);\n  height: min(660px, 90vh);");
    assert_eq!((t.sheet_w, t.sheet_h), (960.0, 660.0));
    sheet_has(".ks-side {\n  flex: none;\n  width: 224px;");
    assert_eq!(t.sheet_side_w, 224.0);
    sheet_has(".ap-folders {\n  flex: none;\n  max-height: 180px;");
    assert_eq!(t.folders_max_h, 180.0);
}

#[test]
fn the_questions_tile_and_callout_shares_are_the_rules_they_were_written_from() {
    let t = ledger::tokens();
    sheet_has("background: color-mix(in srgb, var(--danger) 14%, transparent);\n  color: var(--danger);");
    assert_eq!(t.share_danger_tile, 0.14);
    sheet_has("background: color-mix(in srgb, var(--attention) 12%, transparent);\n  color: var(--fg);");
    assert_eq!(t.share_callout, 0.12);
}

#[test]
fn the_add_dialogs_height_is_the_rule_it_was_written_from() {
    sheet_has(".add-dialog {\n  max-height: min(720px, 90vh);");
    assert_eq!(ledger::tokens().add_max_h, 720.0);
}

#[test]
fn the_settings_sheets_sizes_are_the_rules_they_were_written_from() {
    let t = ledger::tokens();
    sheet_has(".dialog.sheet-dialog {
  width: min(1120px, 94vw);
  height: min(760px, 90vh);");
    assert_eq!((t.settings_w, t.settings_h, t.settings_max_w_share), (1120.0, 760.0, 0.94));
    sheet_has(".settings-nav {
  width: 200px;");
    assert_eq!(t.settings_nav_w, 200.0);
    sheet_has("grid-template-columns: repeat(5, minmax(0, 1fr));");
    assert_eq!(t.theme_cards_per_row, 5);
    sheet_has(".themecard .pv {
  background: var(--base);
  height: 56px;
  border-radius: var(--pv-r);");
    sheet_has(".pv .side {
  background: var(--mantle);
  width: 24px;
  border-radius: 3px;");
    sheet_has(".pv .lines span {
  height: 6px;
  border-radius: 3px;");
    assert_eq!((t.preview_h, t.preview_side_w, t.preview_line_h, t.preview_rounding), (56.0, 24.0, 6.0, 3.0));
    sheet_has(".toggle {
  width: 40px;");
    assert_eq!(t.toggle_w, 40.0);
    sheet_has(".opacity input {
  width: 200px;");
    assert_eq!(t.opacity_slider_w, 200.0);
    sheet_has(".settings .setrow .field {
  width: 260px;");
    assert_eq!(t.setting_field_w, 260.0);
    sheet_has("  max-height: 280px;
  overflow-y: auto;
  z-index: var(--z-layer);");
    assert_eq!(t.font_list_max_h, 280.0);
}

#[test]
fn the_larger_view_the_diff_block_the_form_and_the_changed_wash_are_the_rules_they_were_written_from() {
    let t = ledger::tokens();
    sheet_has(".vfig-pct {\n  min-width: 56px;");
    sheet_has(".vfig-source {\n  width: min(40%, 520px);");
    assert_eq!((t.fig_pct_w, t.fig_source_share, t.fig_source_max_w), (56.0, 0.40, 520.0));
    sheet_has(".vdiff-body {\n  flex: none;\n  max-height: 480px;");
    assert_eq!(t.diff_max_h, 480.0);
    sheet_has("grid-template-columns: repeat(auto-fit, minmax(14rem, 1fr));");
    assert_eq!(t.form_col_min, 14.0 * 16.0);
    sheet_has("background: color-mix(in srgb, var(--changed) 28%, transparent);");
    sheet_has("animation: vchanged 2.4s ease-out 1;");
    assert_eq!((t.share_changed, t.motion_flash), (0.28, std::time::Duration::from_millis(2400)));
}
