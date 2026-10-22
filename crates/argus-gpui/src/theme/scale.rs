//! The page's scale as plain numbers: its `:root` defaults, and how a theme's overrides change them.
//!
//! Every field is a `--name` of `argus-desktop/assets/style.css` in
//! snake_case (`--fs-md` is `fs_md`, `--type-body` is `type_body`), holding the
//! value `:root` gives it; a theme's `Set`s are laid over these, as a
//! `:root[data-theme]` block is over `:root`. The few sizes the page writes as
//! a literal in a rule, with no variable, are named at the foot of [`Scale`].

use std::time::Duration;

use gpui_kit::{point, px, BoxShadow, FontWeight, Pixels};

use super::colour::Colour;
use super::define::Set;

/// One of the page's `--type-*` shorthands: weight, size and line height.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Type {
    pub weight: f32,
    pub size: f32,
    /// A multiple of `size`, as the page's unitless `line-height`.
    pub leading: f32,
}

impl Type {
    /// `<weight> <size>px/<leading>`.
    pub const fn new(weight: f32, size: f32, leading: f32) -> Self {
        Self { weight, size, leading }
    }

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

/// One layer of a `--shadow-*`: `<dx>px <dy>px <blur>px <spread>px <colour>`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layer {
    pub dx: f32,
    pub dy: f32,
    pub blur: f32,
    pub spread: f32,
    pub colour: Colour,
}

impl Layer {
    /// A black drop shadow, the shape of every `--shadow-*` but a theme's hairline layers.
    pub const fn drop(dy: f32, blur: f32, alpha: f32) -> Self {
        Self { dx: 0.0, dy, blur, spread: 0.0, colour: Colour::rgba(0, 0, 0, alpha) }
    }

    pub fn shadow(&self) -> BoxShadow {
        BoxShadow {
            color: self.colour.hsla(),
            offset: point(px(self.dx), px(self.dy)),
            blur_radius: px(self.blur),
            spread_radius: px(self.spread),
            inset: false,
        }
    }
}

/// `--mark-sel`: what a selected row carries on top of `--bg-selected`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    /// Nothing: the fill is the whole selection.
    None,
    /// `inset 0 0 0 1px var(--line-strong)`.
    Hairline,
    /// `inset 3px 0 0 var(--ring)`: a bar down the leading edge.
    Bar,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scale {
    /// `--font-ui-bundled`'s first family, and `--font-mono-bundled`'s.
    pub font_ui: &'static str,
    pub font_mono: &'static str,

    pub fs_2xs: f32,
    pub fs_xs: f32,
    pub fs_sm: f32,
    pub fs_md: f32,
    pub fs_lg: f32,
    pub fs_xl: f32,
    pub fs_2xl: f32,

    pub weight_regular: f32,
    pub weight_medium: f32,
    pub weight_semibold: f32,
    pub weight_bold: f32,

    pub type_display: Type,
    pub type_hero: Type,
    pub type_title: Type,
    pub type_heading: Type,
    pub type_body: Type,
    pub type_small: Type,
    pub type_label: Type,
    pub type_mono: Type,

    pub sp_2: f32,
    pub sp_4: f32,
    pub sp_6: f32,
    pub sp_8: f32,
    pub sp_12: f32,
    pub sp_16: f32,
    pub sp_20: f32,
    pub sp_24: f32,
    pub sp_32: f32,
    pub sp_44: f32,

    pub r_xs: f32,
    pub r_sm: f32,
    pub r_md: f32,
    pub r_lg: f32,
    pub r_pill: f32,

    pub shadow_pop: &'static [Layer],
    pub shadow_dialog: &'static [Layer],
    pub mark_sel: Mark,
    pub opacity_dim: f32,
    /// What a dock or console keeps while another is held.
    pub opacity_held: f32,

    pub control_h: f32,
    pub control_h_sm: f32,
    pub row_h: f32,
    pub row_h_sm: f32,
    pub chip_h: f32,
    pub icon_box: f32,
    pub icon_sm: f32,
    pub icon_lg: f32,
    pub icon_btn: f32,
    pub check_box: f32,
    pub dot_sm: f32,
    pub dot_md: f32,
    pub dot_lg: f32,
    pub meter_h: f32,
    pub header_h: f32,
    pub strip_h: f32,
    pub pane_head_h: f32,
    /// `--rail-w` before the person drags it.
    pub rail_w: f32,
    pub rail_folded: f32,
    pub rail_tile: f32,

    pub t_fast: Duration,

    // The sizes below have no variable: the stylesheet writes each into one
    // rule, and the port names it here so a view holds no bare number for it.
    /// `.sdot`: the box a status dot, ring or dash sits in, and the dash's height.
    pub status_box: f32,
    pub status_dash_h: f32,
    /// A nav row's icon cell, the indent of a checkout's panes under its branch, and the open pane's edge mark.
    pub nav_icon_cell: f32,
    pub pane_indent: f32,
    pub pane_mark_w: f32,
    /// A window control (minimise, maximise, close) is this wide.
    pub window_control_w: f32,
    /// A dialog the window opens without a surface of its own (a rename, a stand-in) is this wide.
    pub dialog_w: f32,
    /// `.pbar`'s width, and `.ghost`'s height.
    pub meter_w: f32,
    pub ghost_h: f32,
    /// `.kbd`'s line height and `.fbtn`'s height.
    pub key_leading: f32,
    pub fold_btn_h: f32,
    /// The focus ring: a `:focus-visible` outline of this width in `--ring`, the
    /// halo (`--ring-shadow`) of this width at this share of the ring colour.
    pub ring_outline: f32,
    pub ring_halo_width: f32,
    pub ring_halo_share: f32,
    // The shares the page's `color-mix(in srgb, <colour> N%, transparent)` and
    // `color-mix(<a> N%, <b>)` rules use, by the rule they serve. A view washes
    // its own colour with one; a derived role in `derive` washes the theme's.
    /// `.btn:hover` and `.btn:active`: the text colour at these shares.
    pub share_hover: f32,
    pub share_press: f32,
    /// An icon button's hover over `--fill`, and its press, which a lit console head also lays its fill with.
    pub share_hover_icon: f32,
    pub share_press_icon: f32,
    /// `.btn` hover's edge: `--line-strong` this far toward the accent; a cap on a primary button, its edge in the on-accent colour.
    pub share_edge: f32,
    /// A diff side's added or removed wash, and the picked lines laid over it.
    pub share_tone: f32,
    pub share_pick: f32,
    /// A sash's grab zone under the pointer: the accent at this share. The port's own; the page's review has no sash.
    pub share_sash: f32,
    /// The accent a dock's drop landing is washed with, and the kit's drop target.
    pub share_landing: f32,
    pub share_drop_target: f32,
    /// `.rfile.on:hover`: the text colour over the selected colour.
    pub share_selected_hover: f32,
    /// The kit's text selection, as a share of the accent.
    pub share_selection: f32,
    /// The kit's backdrop behind a modal: black at this alpha.
    pub share_overlay_dim: f32,
    /// `--bg-raised` mixes this much `--bg-surface` with `--fill`.
    pub share_raised: f32,
    /// `--fg-fainter` when a theme does not pin it: `--fg-faint` this far toward `--bg-sunken`.
    pub share_fainter: f32,
    /// How far a kit hover or press steps a filled colour lighter or darker
    /// (the palette has one raised and one selected grey and a button needs
    /// three), and a link's.
    pub step_lift: f32,
    pub step_link: f32,
    /// The chevron's turn, a spinner's turn (and the slow one), the placeholder
    /// shimmer and the changed-file flash. There are no custom easings:
    /// transitions use `ease`, the flash `ease-out`, the shimmer `ease-in-out`
    /// and spinners `linear`.
    pub motion_turn: Duration,
    pub motion_spin: Duration,
    pub motion_spin_slow: Duration,
    pub motion_shimmer: Duration,
    pub motion_flash: Duration,
}

const ROOT_POP: &[Layer] = &[Layer::drop(10.0, 30.0, 0.35)];
const ROOT_DIALOG: &[Layer] = &[Layer::drop(24.0, 64.0, 0.45)];

impl Scale {
    /// `:root`, which every theme starts from.
    pub const fn root() -> Self {
        Self {
            font_ui: "Geist",
            font_mono: "JetBrains Mono",

            fs_2xs: 10.0,
            fs_xs: 11.0,
            fs_sm: 12.0,
            fs_md: 13.0,
            fs_lg: 15.0,
            fs_xl: 17.0,
            fs_2xl: 20.0,

            weight_regular: 400.0,
            weight_medium: 500.0,
            weight_semibold: 600.0,
            weight_bold: 700.0,

            type_display: Type::new(650.0, 20.0, 1.3),
            type_hero: Type::new(700.0, 28.0, 1.2),
            type_title: Type::new(650.0, 17.0, 1.3),
            type_heading: Type::new(600.0, 15.0, 1.35),
            type_body: Type::new(400.0, 13.0, 1.45),
            type_small: Type::new(400.0, 12.0, 1.45),
            type_label: Type::new(700.0, 11.0, 1.4),
            // `400 13px/20px`: the one with an absolute line height.
            type_mono: Type::new(400.0, 13.0, 20.0 / 13.0),

            sp_2: 2.0,
            sp_4: 4.0,
            sp_6: 6.0,
            sp_8: 8.0,
            sp_12: 12.0,
            sp_16: 16.0,
            sp_20: 20.0,
            sp_24: 24.0,
            sp_32: 32.0,
            sp_44: 44.0,

            r_xs: 4.0,
            r_sm: 6.0,
            r_md: 10.0,
            r_lg: 14.0,
            // The page's buttons and chips are pills.
            r_pill: 999.0,

            shadow_pop: ROOT_POP,
            shadow_dialog: ROOT_DIALOG,
            mark_sel: Mark::Bar,
            opacity_dim: 0.55,
            opacity_held: 0.5,

            control_h: 30.0,
            control_h_sm: 24.0,
            row_h: 28.0,
            row_h_sm: 26.0,
            chip_h: 20.0,
            icon_box: 20.0,
            icon_sm: 14.0,
            icon_lg: 24.0,
            icon_btn: 28.0,
            check_box: 16.0,
            dot_sm: 6.0,
            dot_md: 7.0,
            dot_lg: 8.0,
            meter_h: 6.0,
            header_h: 56.0,
            strip_h: 40.0,
            pane_head_h: 32.0,
            rail_w: 288.0,
            rail_folded: 56.0,
            rail_tile: 40.0,

            t_fast: Duration::from_millis(120),

            status_box: 9.0,
            status_dash_h: 1.5,
            nav_icon_cell: 22.0,
            pane_indent: 38.0,
            pane_mark_w: 2.0,
            window_control_w: 46.0,
            dialog_w: 420.0,
            meter_w: 56.0,
            ghost_h: 12.0,
            key_leading: 14.0,
            fold_btn_h: 22.0,
            ring_outline: 2.0,
            ring_halo_width: 3.0,
            ring_halo_share: 0.30,
            share_hover: 0.07,
            share_press: 0.12,
            share_hover_icon: 0.10,
            share_press_icon: 0.15,
            share_edge: 0.45,
            share_tone: 0.12,
            share_pick: 0.24,
            share_sash: 0.4,
            share_landing: 0.14,
            share_drop_target: 0.18,
            share_selected_hover: 0.06,
            share_selection: 0.30,
            share_overlay_dim: 0.55,
            share_raised: 0.55,
            share_fainter: 0.65,
            step_lift: 0.06,
            step_link: 0.08,
            motion_turn: Duration::from_millis(150),
            motion_spin: Duration::from_millis(900),
            motion_spin_slow: Duration::from_millis(1000),
            motion_shimmer: Duration::from_millis(1400),
            motion_flash: Duration::from_millis(2400),
        }
    }

    /// Lays one override over this scale. The colour overrides are the next
    /// module's, since they apply to roles derived from the palette.
    pub fn set(&mut self, set: &Set) {
        match *set {
            Set::FontUi(family) => self.font_ui = family,
            Set::FontMono(family) => self.font_mono = family,
            Set::RXs(v) => self.r_xs = v,
            Set::RSm(v) => self.r_sm = v,
            Set::RMd(v) => self.r_md = v,
            Set::RLg(v) => self.r_lg = v,
            Set::RPill(v) => self.r_pill = v,
            Set::ControlH(v) => self.control_h = v,
            Set::ControlHSm(v) => self.control_h_sm = v,
            Set::IconBtn(v) => self.icon_btn = v,
            Set::TypeDisplay(v) => self.type_display = v,
            Set::TypeHero(v) => self.type_hero = v,
            Set::TypeTitle(v) => self.type_title = v,
            Set::TypeHeading(v) => self.type_heading = v,
            Set::TypeBody(v) => self.type_body = v,
            Set::TypeSmall(v) => self.type_small = v,
            Set::TypeLabel(v) => self.type_label = v,
            Set::TypeMono(v) => self.type_mono = v,
            Set::ShadowPop(v) => self.shadow_pop = v,
            Set::ShadowDialog(v) => self.shadow_dialog = v,
            Set::MarkSel(v) => self.mark_sel = v,
            Set::BgRaised(_) | Set::Fill(_) | Set::Line(_) | Set::LineStrong(_) | Set::Ring(_) | Set::Scrim(_) | Set::FgFainter(_) => {}
        }
    }

    /// `--shadow-pop` as the layers GPUI draws.
    pub fn shadow_pop(&self) -> Vec<BoxShadow> {
        self.shadow_pop.iter().map(Layer::shadow).collect()
    }

    /// `--shadow-dialog` as the layers GPUI draws.
    pub fn shadow_dialog(&self) -> Vec<BoxShadow> {
        self.shadow_dialog.iter().map(Layer::shadow).collect()
    }
}
