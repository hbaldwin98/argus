//! The page's scale as plain numbers: its `:root` defaults, and how a theme's overrides change them.
//!
//! Every field is a `--name` of `argus-desktop/assets/style.css` in
//! snake_case (`--fs-md` is `fs_md`, `--type-body` is `type_body`), holding the
//! value `:root` gives it; a theme's `Set`s are laid over these, as a
//! `:root[data-theme]` block is over `:root`. The few sizes the page writes as
//! a literal in a rule, with no variable, are named at the foot of [`Scale`].

use std::time::Duration;

use gpui_kit::{point, px, BoxShadow, FontWeight, Rems};

use super::colour::Colour;
use super::define::Set;

/// What one rem is at 100%: `--fs-md`, which is the kit's own base (its root sets the window's rem size to the theme's font
/// size on every frame). A view's size is `rem(token)`, so the zoom, which is that font size, scales it.
pub const REM: f32 = 13.0;

/// A token as a length that follows the zoom: `v` pixels at 100%.
pub fn rem(v: f32) -> gpui_kit::Rems {
    gpui_kit::rems(v / REM)
}

/// A token as pixels at the window's zoom, for where a length must be `Pixels`: a dialog's width, a
/// hit test, the kit's own `rounded`. `rem_size` is the window's (`window.rem_size()`).
pub fn pixels(v: f32, rem_size: gpui_kit::Pixels) -> gpui_kit::Pixels {
    rem(v).to_pixels(rem_size)
}

/// How far the window is zoomed, 1.0 at 100%: its rem size over what a rem is at 100%.
pub fn zoom(rem_size: gpui_kit::Pixels) -> f32 {
    f32::from(rem_size) / REM
}

/// What a length of the window's real pixels is in the page's: the unit the session's sizes (a
/// dock's, the rail's, a floating panel's), the breakpoints and the drags are in.
pub fn css(real: gpui_kit::Pixels, rem_size: gpui_kit::Pixels) -> f32 {
    f32::from(real) / zoom(rem_size)
}

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

    /// The size and the line height are rems of the scale, so the zoom scales type with the rest.
    pub fn size(&self) -> Rems {
        rem(self.size)
    }

    pub fn line_height(&self) -> Rems {
        rem(self.size * self.leading)
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
    /// The theme's own faces, which a family the person chose goes in front of: what draws the
    /// glyphs the chosen one lacks, and what an empty font field says it draws.
    pub font_ui_bundled: &'static str,
    pub font_mono_bundled: &'static str,

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
    /// The width of a hairline border.
    pub hairline: f32,
    /// The width of an accent edge or lit line: a keyed header's underline, a gutter's grip.
    pub rule: f32,

    pub r_xs: f32,
    pub r_sm: f32,
    pub r_md: f32,
    pub r_lg: f32,
    pub r_pill: f32,

    pub shadow_pop: &'static [Layer],
    pub shadow_dialog: &'static [Layer],
    pub mark_sel: Mark,
    pub opacity_dim: f32,
    /// `.vact .btn[disabled]`: an action waiting on its agent, or one the daemon cannot take.
    pub opacity_wait: f32,
    /// What a dock or console keeps while another is held.
    pub opacity_held: f32,

    pub control_h: f32,
    pub control_h_sm: f32,
    pub row_h: f32,
    pub row_h_sm: f32,
    pub chip_h: f32,
    pub icon_box: f32,
    pub icon_xs: f32,
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
    /// `--icon-glyph`: the glyph of a folded rail's button; `--drawer-max`: the widest the right dock opens over the stage in a narrow window.
    pub icon_glyph: f32,
    /// `.menu`: the width of a menu drawn in a row, and how far under the row's top it opens.
    pub menu_w: f32,
    pub menu_top: f32,
    pub drawer_max: f32,
    /// `--dock-floor`, `--dock-min-pane`, `--dock-room`: what a side dock leaves of the work area at least; the right dock's width past which it needs the room; and the room it leaves the pane above `narrow_below`.
    pub dock_floor: f32,
    pub dock_min_pane: f32,
    pub dock_room: f32,

    pub t_fast: Duration,

    // The sizes below have no variable: the stylesheet writes each into one
    // rule, and the port names it here so a view holds no bare number for it.
    /// `.sdot`: the box a status dot, ring or dash sits in, and the dash's height.
    pub status_box: f32,
    pub status_dash_h: f32,
    /// A nav row's icon cell, the indent of a checkout's panes under its branch, and the open pane's edge mark.
    pub nav_icon_cell: f32,
    /// `.pane`, `.nav` and `.search`: the height of a pane row, a nav row and the search field.
    pub pane_row_h: f32,
    pub nav_h: f32,
    pub search_h: f32,
    /// `.nav-saved-list`: the share of the window's height the Views fold's list may take before it scrolls (`40vh`).
    pub nav_saved_share: f32,
    pub pane_indent: f32,
    pub pane_mark_w: f32,
    /// `inset 3px 0 0`: the bar a selected row carries under a theme whose `--mark-sel` is one.
    pub mark_bar_w: f32,
    /// A window control (minimise, maximise, close) is this wide.
    pub window_control_w: f32,
    /// A dialog the window opens without a surface of its own (a rename, a stand-in) is this wide.
    pub dialog_w: f32,
    /// `.pbar`'s width, and `.ghost`'s height.
    pub meter_w: f32,
    /// `.vmeter-sm`, the meter beside a number in a cell or a field.
    pub meter_sm_w: f32,
    /// The blocks of an agent's view. `.vcolumns` and `.vgrid` (`sm`, `md` and `lg` are the names an agent picks a minimum by): the narrowest a column or a grid item is.
    pub col_min: f32,
    pub grid_min_sm: f32,
    pub grid_min_md: f32,
    pub grid_min_lg: f32,
    /// `.vstat-value`.
    pub type_stat: Type,
    /// `.vtl-event` and `.vtl-dot`: the time column, the dot, the rule the dots hang on, and the window width under which the lanes stack (`@media (max-width: 640px)`).
    pub tl_at_w: f32,
    pub tl_dot: f32,
    pub tl_rule_w: f32,
    /// Where the rule starts under an event's top, and how far it runs past its bottom, to meet the next event's.
    pub tl_rule_top: f32,
    pub tl_rule_bleed: f32,
    pub tl_stack_below: f32,
    /// `.vcode-bd` and `.vlog`: how tall each grows before it scrolls, and a code line's height.
    pub code_max_h: f32,
    pub code_line_h: f32,
    pub log_max_h: f32,
    /// `.vmatrix-cell` and `.vmatrix`'s spacing; `.vboard-col`; `.vfields`'s label column; `.vlist-item`'s lead column and the indent of what follows a person; `.vspin`; and a checkbox's edge.
    pub matrix_cell_w: f32,
    pub matrix_cell_h: f32,
    pub matrix_gap: f32,
    pub board_col_w: f32,
    pub fields_label_w: f32,
    pub list_lead_w: f32,
    pub list_indent: f32,
    pub spin_sm: f32,
    pub check_edge: f32,
    pub ghost_h: f32,
    /// `.vchart-svg`: the most a chart is wide and tall; `.vchart-line` and `.vchart-key`: a line's stroke, which does not scale with the chart.
    pub chart_max_w: f32,
    pub chart_max_h: f32,
    pub chart_stroke: f32,
    /// `.vspark`: a spark's height, a stat's, and how wide a spark chart gets.
    pub spark_h: f32,
    pub stat_spark_h: f32,
    pub spark_max_w: f32,
    /// `.vtable-table td`, `td.vtable-long` and `.vtable-filter`: a row's height, how wide a long text cell grows before it is cut, and the filter's narrowest.
    pub table_row_h: f32,
    pub table_long_max_w: f32,
    pub table_filter_min_w: f32,
    /// `.vd-scroll svg`: the sequence diagram's type, and the number in a step's dot (`.vd-num`).
    pub vd_font: f32,
    pub vd_num_font: f32,
    /// The sequence diagram's strokes: boxes and the dot (`.vd-box`), a message (`.vd-msg`), an actor (`.vd-actor`); the dashes of a lifeline (`4 4`, `.vd-life`) and of a reply (`5 4`, `.vd-reply`); the size of an arrowhead (`markerWidth` of the page's marker), and the corners of a box and of a branch.
    pub vd_stroke: f32,
    pub vd_msg_stroke: f32,
    pub vd_actor_stroke: f32,
    pub vd_life_dash: f32,
    pub vd_reply_on: f32,
    pub vd_reply_off: f32,
    pub vd_head: f32,
    pub vd_box_radius: f32,
    pub vd_branch_radius: f32,
    /// The graph's strokes: a box (`.vg-box`) and an edge (`.vg-edge`); an edge label's type and its halo (`.vg-elabel`); an arrowhead (`markerWidth`, in units of the edge's stroke); and a box's corners.
    pub vg_stroke: f32,
    pub vg_edge_stroke: f32,
    pub vg_label_font: f32,
    pub vg_node_font: f32,
    pub vg_halo: f32,
    pub vg_head: f32,
    pub vg_radius: f32,
    /// `.vfig-pct`'s least width, `.vfig-source`'s share of the larger view and its widest (`min(40%, 520px)`), and `.vdiff-body`'s tallest before the diff scrolls.
    pub fig_pct_w: f32,
    pub fig_source_share: f32,
    pub fig_source_max_w: f32,
    pub diff_max_h: f32,
    /// `.vform-grid`'s narrowest column (`14rem`), and the share of the changed colour a block's wash starts at (`@keyframes vchanged`).
    pub form_col_min: f32,
    pub share_changed: f32,
    /// A terminal selection is the text colour at this share (`panes/screen.ts`, `drawSelection`).
    pub share_term_selection: f32,
    /// The stage strip drops the pane's size label when the window is narrower than this (`.head-size`).
    pub head_size_hidden_below: f32,
    /// The window is narrow at this width and under: the rail folds, the right dock is a drawer, and the strip says the pane's status itself (`@media (max-width: 1200px)`).
    pub narrow_below: f32,
    /// `--tile`, and `--dialog-w-sm`: the empty stage's icon tile and the Stopped card's width.
    pub tile: f32,
    pub dialog_w_sm: f32,
    /// `--dialog-w` (a form such as Add a project), the palette's width, the shortcuts sheet's width and height and its side column, and how tall the add dialog's folder list may grow before it scrolls (`.palette`, `.keys-sheet`, `.ks-side`, `.ap-folders`).
    pub form_w: f32,
    pub palette_w: f32,
    pub sheet_w: f32,
    pub sheet_h: f32,
    /// How tall the palette and the sheet may grow, and the palette's top gap, as shares of the window (`.palette`, `.scrim.top`, `.keys-sheet`).
    pub palette_max_share: f32,
    pub palette_top_share: f32,
    pub sheet_max_share: f32,
    pub sheet_side_w: f32,
    pub folders_max_h: f32,
    /// `.dialog.sheet-dialog` (the Settings sheet: its width and height, the share of the window each may take, which the shortcuts sheet's own height share is also), `.settings-nav`, `.themecards` and `.themecard .pv` (the cards per row, the preview's height, its sidebar's width, a line's height and the rounding of both), `.toggle`, `.opacity input` and the percent beside it, `.settings .setrow .field` and `.fontpick-list`.
    pub settings_w: f32,
    pub settings_h: f32,
    pub settings_max_w_share: f32,
    pub settings_nav_w: f32,
    pub theme_cards_per_row: usize,
    pub preview_h: f32,
    pub preview_side_w: f32,
    pub preview_line_h: f32,
    pub preview_rounding: f32,
    pub toggle_w: f32,
    pub opacity_slider_w: f32,
    pub percent_w: f32,
    pub setting_field_w: f32,
    pub font_list_max_h: f32,
    /// `.add-dialog`: how tall the add dialog may grow before its body scrolls.
    pub add_max_h: f32,
    /// `.empty-body` and its `no-projects` and `no-pane` forms: how wide each empty stage's block is, and the Stopped card's icon.
    pub empty_w: f32,
    pub empty_w_projects: f32,
    pub empty_w_pane: f32,
    pub stopped_icon: f32,
    /// The Connecting stage's spinner, drawn at 40 by the page's empty-stage icon.
    pub empty_spinner: f32,
    /// `--vstat-min`: how narrow a card of the panes to go back to is allowed to be; and `.rcard`'s hover and press, the text colour over the raised one.
    pub card_min: f32,
    pub share_card_hover: f32,
    pub share_card_press: f32,
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
    /// The bin tile of a destructive question (`.danger-tile`) and the callout under it (`.warn-callout`): the danger and the attention colour at these shares.
    pub share_danger_tile: f32,
    pub share_callout: f32,
    /// `.vcard.tinted`: a card's edge is its tone this far toward the line colour.
    pub share_card_edge: f32,
    /// A tone's wash behind what is said in it: the meter's track (`.vmeter`), the chip (`.vchip`)
    /// and the avatar on the fill (`.vav`).
    pub share_meter_track: f32,
    pub share_chip: f32,
    pub share_avatar: f32,
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
            font_ui_bundled: "Geist",
            font_mono_bundled: "JetBrains Mono",

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
            hairline: 1.0,
            rule: 2.0,

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
            opacity_wait: 0.7,
            opacity_held: 0.5,

            control_h: 30.0,
            control_h_sm: 24.0,
            row_h: 28.0,
            row_h_sm: 26.0,
            chip_h: 20.0,
            icon_box: 20.0,
            icon_xs: 12.0,
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
            icon_glyph: 16.0,
            menu_w: 210.0,
            menu_top: 28.0,
            drawer_max: 560.0,
            dock_floor: 240.0,
            dock_min_pane: 440.0,
            dock_room: 800.0,

            t_fast: Duration::from_millis(120),

            status_box: 9.0,
            status_dash_h: 1.5,
            nav_icon_cell: 22.0,
            pane_row_h: 32.0,
            nav_h: 32.0,
            search_h: 34.0,
            nav_saved_share: 0.40,
            pane_indent: 38.0,
            pane_mark_w: 2.0,
            mark_bar_w: 3.0,
            window_control_w: 46.0,
            dialog_w: 420.0,
            meter_w: 56.0,
            meter_sm_w: 60.0,
            col_min: 240.0,
            grid_min_sm: 160.0,
            grid_min_md: 240.0,
            grid_min_lg: 360.0,
            type_stat: Type::new(650.0, 22.0, 1.2),
            tl_at_w: 84.0,
            tl_dot: 12.0,
            tl_rule_w: 2.0,
            tl_rule_top: 14.0,
            tl_rule_bleed: 2.0,
            tl_stack_below: 640.0,
            code_max_h: 360.0,
            code_line_h: 18.0,
            log_max_h: 320.0,
            matrix_cell_w: 44.0,
            matrix_cell_h: 32.0,
            matrix_gap: 2.0,
            board_col_w: 240.0,
            fields_label_w: 132.0,
            list_lead_w: 20.0,
            list_indent: 28.0,
            spin_sm: 10.0,
            check_edge: 1.5,
            ghost_h: 12.0,
            chart_max_w: 720.0,
            chart_max_h: 360.0,
            chart_stroke: 2.0,
            spark_h: 32.0,
            stat_spark_h: 28.0,
            spark_max_w: 240.0,
            table_row_h: 34.0,
            table_long_max_w: 280.0,
            table_filter_min_w: 140.0,
            vd_font: 12.0,
            vd_num_font: 10.0,
            vd_stroke: 1.0,
            vd_msg_stroke: 1.4,
            vd_actor_stroke: 1.5,
            vd_life_dash: 4.0,
            vd_reply_on: 5.0,
            vd_reply_off: 4.0,
            vd_head: 9.0,
            vd_box_radius: 5.0,
            vd_branch_radius: 3.0,
            vg_stroke: 1.5,
            vg_edge_stroke: 1.3,
            vg_label_font: 11.0,
            vg_node_font: 12.0,
            vg_halo: 4.0,
            vg_head: 8.0,
            vg_radius: 6.0,
            fig_pct_w: 56.0,
            fig_source_share: 0.40,
            fig_source_max_w: 520.0,
            diff_max_h: 480.0,
            form_col_min: 224.0,
            share_changed: 0.28,
            share_term_selection: 0.30,
            head_size_hidden_below: 1100.0,
            narrow_below: 1200.0,
            tile: 56.0,
            dialog_w_sm: 440.0,
            form_w: 560.0,
            palette_w: 620.0,
            sheet_w: 960.0,
            sheet_h: 660.0,
            palette_max_share: 0.70,
            palette_top_share: 0.12,
            sheet_max_share: 0.90,
            sheet_side_w: 224.0,
            folders_max_h: 180.0,
            settings_w: 1120.0,
            settings_h: 760.0,
            settings_max_w_share: 0.94,
            settings_nav_w: 200.0,
            theme_cards_per_row: 5,
            preview_h: 56.0,
            preview_side_w: 24.0,
            preview_line_h: 6.0,
            preview_rounding: 3.0,
            toggle_w: 40.0,
            opacity_slider_w: 200.0,
            percent_w: 36.0,
            setting_field_w: 260.0,
            font_list_max_h: 280.0,
            add_max_h: 720.0,
            empty_w: 520.0,
            empty_w_projects: 460.0,
            empty_w_pane: 760.0,
            stopped_icon: 28.0,
            empty_spinner: 40.0,
            card_min: 200.0,
            share_card_hover: 0.06,
            share_card_press: 0.10,
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
            share_danger_tile: 0.14,
            share_callout: 0.12,
            share_card_edge: 0.55,
            share_meter_track: 0.18,
            share_chip: 0.16,
            share_avatar: 0.24,
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
            Set::FontUi(family) => (self.font_ui, self.font_ui_bundled) = (family, family),
            Set::FontMono(family) => (self.font_mono, self.font_mono_bundled) = (family, family),
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
