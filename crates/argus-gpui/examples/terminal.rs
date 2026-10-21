//! Can a custom GPUI `Element` draw a pane's screen from the rows the daemon sends, fast enough?
//!
//! The terminal screen is the largest surface the port has to carry (see
//! GPUI.md), and it is the one the kit has no component for. This spike
//! answers it the way Zed's `terminal_element` does: measure one mono cell,
//! turn each row of segments into runs of same-styled text and into merged
//! background rectangles, and paint those as a handful of quads and shaped
//! lines. `Frame`, `Segment` and `FrameCursor` are local copies of the
//! shapes in `argus-desktop`'s `connection/page.rs`, so the spike does not
//! depend on that crate; the flags and the packed colours are the same.
//!
//! Run it with `cargo run -p argus-gpui --example terminal [-- --seconds N]`.
//! It shows a 140x40 screen, repaints a fresh full frame every 16 ms, and
//! prints the paint cost and the grid the element reported on exit.

#![cfg_attr(all(not(debug_assertions), windows), windows_subsystem = "windows")]

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use argus_gpui::theme::Ledger;
use argus_gpui::window;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{
    div, fill, outline, point, prelude::*, px, relative, rgb, size, App, Bounds, BorderStyle,
    ContentMask, Context, Element, ElementId, Font, FontStyle, FontWeight, GlobalElementId, Hsla,
    InspectorElementId, IntoElement, LayoutId, Pixels, Render, SharedString, Style, Task,
    TextAlign, TextRun, UnderlineStyle, Window,
};

// The terminal's type as the Tauri page sets it: 13px on a 20px line.
const FONT_SIZE: f32 = 13.0;
const LINE_HEIGHT: f32 = 20.0;

const FRAME_INTERVAL: Duration = Duration::from_millis(16);
const DEFAULT_SECONDS: u64 = 10;

// A segment's flags, as `connection/page.rs` packs them.
const BOLD: u8 = 1;
const ITALIC: u8 = 2;
const UNDERLINE: u8 = 4;
const REVERSE: u8 = 8;
const WIDE: u8 = 16;

// ---------------------------------------------------------------------------
// The wire shapes
// ---------------------------------------------------------------------------

/// A stretch of one style along a row: `[col, cells, fg, bg, flags]`. Colours
/// are packed as on the wire, `0` the default, `1 << 24 | index`,
/// `2 << 24 | rgb`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Segment(u16, Vec<String>, u32, u32, u8);

impl Segment {
    /// A wide cell is sent alone, with no spacer after it, and covers two columns.
    fn width(&self) -> u16 {
        if self.4 & WIDE != 0 {
            2
        } else {
            self.1.len() as u16
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CursorShape {
    Block,
    Underline,
    Bar,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FrameCursor {
    row: u16,
    col: u16,
    visible: bool,
    shape: CursorShape,
}

/// The rows of a screen that changed since the last frame, or all of them.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Frame {
    rows: u16,
    cols: u16,
    full: bool,
    cursor: FrameCursor,
    lines: Vec<(u16, Vec<Segment>)>,
}

/// What the page mirrors: every row's segments, so a repaint needs no new frame.
#[derive(Debug, Clone)]
struct Screen {
    rows: u16,
    cols: u16,
    lines: Vec<Vec<Segment>>,
    cursor: FrameCursor,
}

impl Screen {
    fn new() -> Self {
        Self {
            rows: 0,
            cols: 0,
            lines: Vec::new(),
            cursor: FrameCursor { row: 0, col: 0, visible: false, shape: CursorShape::Block },
        }
    }

    /// A row in `frame.lines` replaces the row held, whole; a full frame or a
    /// resize first drops everything held, as the page does.
    fn apply(&mut self, frame: Frame) {
        if frame.full || frame.rows != self.rows || frame.cols != self.cols {
            self.lines = vec![Vec::new(); frame.rows as usize];
        }
        self.rows = frame.rows;
        self.cols = frame.cols;
        for (row, segments) in frame.lines {
            if let Some(slot) = self.lines.get_mut(row as usize) {
                *slot = segments;
            }
        }
        self.cursor = frame.cursor;
    }

    /// The glyph at a cell, for the cursor to redraw in the background colour.
    fn glyph_at(&self, row: u16, col: u16) -> &str {
        for seg in self.lines.get(row as usize).into_iter().flatten() {
            if col >= seg.0 && col < seg.0 + seg.width() {
                let i = if seg.4 & WIDE != 0 { 0 } else { (col - seg.0) as usize };
                return seg.1.get(i).map_or(" ", String::as_str);
            }
        }
        " "
    }
}

// ---------------------------------------------------------------------------
// Colour
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Rgb(u8, u8, u8);

impl Rgb {
    fn hsla(self) -> Hsla {
        rgb(u32::from(self.0) << 16 | u32::from(self.1) << 8 | u32::from(self.2)).into()
    }
}

struct Palette {
    fg: Rgb,
    bg: Rgb,
    ansi: [Rgb; 16],
}

impl Palette {
    /// The Ledger terminal: the 16 ANSI slots drawn from its semantic colours.
    fn ledger() -> Self {
        let c = |hex: u32| Rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8);
        Self {
            fg: c(0xf7f8f8),
            bg: c(0x08090a),
            ansi: [
                c(0x1e1f25), c(0xf07a7a), c(0x4cb782), c(0xf2c94c),
                c(0x4ea7fc), c(0xb48cff), c(0x26b5ce), c(0xb4bac3),
                c(0x5c6068), c(0xff9a9a), c(0x6fd9a0), c(0xffe08a),
                c(0x7cc0ff), c(0xa0a8ff), c(0x5fd3e8), c(0xf7f8f8),
            ],
        }
    }
}

/// One level of the 256-colour cube.
fn cube_step(c: u32) -> u8 {
    if c == 0 { 0 } else { (55 + 40 * c) as u8 }
}

/// A packed wire colour; `None` is the default, which the caller knows.
fn resolve(packed: u32, palette: &Palette) -> Option<Rgb> {
    match packed >> 24 {
        0 => None,
        2 => Some(Rgb((packed >> 16) as u8, (packed >> 8) as u8, packed as u8)),
        _ => {
            let i = packed & 255;
            Some(if i < 16 {
                palette.ansi[i as usize]
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

fn luminance(c: Rgb) -> f32 {
    let linear = |v: u8| {
        let s = f32::from(v) / 255.0;
        if s <= 0.03928 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * linear(c.0) + 0.7152 * linear(c.1) + 0.0722 * linear(c.2)
}

fn contrast(a: Rgb, b: Rgb) -> f32 {
    let (x, y) = (luminance(a), luminance(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

/// WCAG's floor for body text, the same one `panes/contrast.ts` holds: a
/// program that paints a red or green background picks its text for its own
/// palette, and the theme's text can be as dark as that background.
const MINIMUM_CONTRAST: f32 = 4.5;

/// `fore` on `back`, moved toward black or white, whichever reads better on
/// `back`, until it reaches the floor.
fn readable(fore: Rgb, back: Rgb) -> Rgb {
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

// ---------------------------------------------------------------------------
// From rows to what is painted
// ---------------------------------------------------------------------------

/// What makes two stretches of text one shaped line. The background is not
/// here: it is painted as quads, so it never splits a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TextStyle {
    fg: Rgb,
    bold: bool,
    italic: bool,
    underline: bool,
}

/// Contiguous cells of one style, shaped and painted as one line.
#[derive(Debug, Clone, PartialEq, Eq)]
struct TextBatch {
    row: u16,
    col: u16,
    cols: u16,
    text: String,
    style: TextStyle,
    /// A wide cell is its own batch, since its glyph is forced two cells wide.
    wide: bool,
}

/// A rectangle of cells in one background colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Fill {
    row: u16,
    col: u16,
    cols: u16,
    rows: u16,
    color: Rgb,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct Layout {
    batches: Vec<TextBatch>,
    fills: Vec<Fill>,
}

/// A segment's text and background after the flags and the contrast floor.
fn segment_colours(seg: &Segment, palette: &Palette) -> (Rgb, Option<Rgb>) {
    let mut fore = resolve(seg.2, palette).unwrap_or(palette.fg);
    let mut back = resolve(seg.3, palette);
    if seg.4 & REVERSE != 0 {
        (fore, back) = (back.unwrap_or(palette.bg), Some(fore));
    }
    if let Some(back) = back {
        fore = readable(fore, back);
    }
    (fore, back)
}

/// Runs and background rectangles for every row. Backgrounds merge along a
/// row where they touch, and down the rows where a rectangle repeats exactly,
/// so a diff's coloured block is one quad rather than one per cell.
fn layout(screen: &Screen, palette: &Palette) -> Layout {
    let mut out = Layout::default();
    // The fills still open: those of the row above, which the next row may extend.
    let mut open: Vec<usize> = Vec::new();
    for (row, segments) in screen.lines.iter().enumerate() {
        let row = row as u16;
        let mut current: Option<TextBatch> = None;
        let mut row_fills: Vec<Fill> = Vec::new();
        for seg in segments {
            if seg.1.is_empty() {
                continue;
            }
            let (fore, back) = segment_colours(seg, palette);
            let width = seg.width();
            if let Some(color) = back {
                match row_fills.last_mut() {
                    Some(last) if last.col + last.cols == seg.0 && last.color == color => {
                        last.cols += width;
                    }
                    _ => row_fills.push(Fill { row, col: seg.0, cols: width, rows: 1, color }),
                }
            }
            let style = TextStyle {
                fg: fore,
                bold: seg.4 & BOLD != 0,
                italic: seg.4 & ITALIC != 0,
                underline: seg.4 & UNDERLINE != 0,
            };
            let wide = seg.4 & WIDE != 0;
            let mut text = String::new();
            for cell in &seg.1 {
                text.push_str(if cell.is_empty() { " " } else { cell });
            }
            match current.as_mut() {
                Some(batch)
                    if !wide && !batch.wide && batch.style == style && batch.col + batch.cols == seg.0 =>
                {
                    batch.text.push_str(&text);
                    batch.cols += width;
                }
                _ => {
                    out.batches.extend(current.take().filter(paints));
                    current = Some(TextBatch { row, col: seg.0, cols: width, text, style, wide });
                }
            }
        }
        out.batches.extend(current.take().filter(paints));

        let mut next_open = Vec::new();
        for fill in row_fills {
            let above = open.iter().copied().find(|&i| {
                let f = &out.fills[i];
                f.col == fill.col && f.cols == fill.cols && f.color == fill.color && f.row + f.rows == row
            });
            match above {
                Some(i) => {
                    out.fills[i].rows += 1;
                    next_open.push(i);
                }
                None => {
                    out.fills.push(fill);
                    next_open.push(out.fills.len() - 1);
                }
            }
        }
        open = next_open;
    }
    out
}

/// A run of blanks with no underline paints nothing; shaping it is waste.
fn paints(batch: &TextBatch) -> bool {
    batch.style.underline || batch.text.chars().any(|c| c != ' ')
}

/// The grid a pixel size holds. The epsilon is for a width that is exactly
/// `n` cells in the font's units and lands a hair under it in `f32`.
fn grid_for(width: f32, height: f32, cell_w: f32, cell_h: f32) -> (u16, u16) {
    let fit = |room: f32, cell: f32| ((room / cell + 1e-3).floor().max(1.0)).min(f32::from(u16::MAX)) as u16;
    (fit(height, cell_h), fit(width, cell_w))
}

// ---------------------------------------------------------------------------
// The element
// ---------------------------------------------------------------------------

#[derive(Default)]
struct PaintStats {
    frames: u32,
    total: Duration,
    worst: Duration,
    /// The wait between one paint starting and the next.
    worst_gap: Duration,
    last_start: Option<Instant>,
    quads: usize,
    batches: usize,
    layout_total: Duration,
}

struct Metrics {
    font: Font,
    cell_w: Pixels,
    cell_h: Pixels,
}

struct TerminalElement {
    screen: Arc<Screen>,
    palette: Rc<Palette>,
    family: SharedString,
    focused: bool,
    /// Where the element says how many rows and columns its size holds, which
    /// the page sends the daemon as a `resize`.
    reported: Rc<Cell<(u16, u16)>>,
    stats: Rc<RefCell<PaintStats>>,
}

impl Element for TerminalElement {
    type RequestLayoutState = ();
    type PrepaintState = Metrics;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = relative(1.0).into();
        style.size.height = relative(1.0).into();
        (window.request_layout(style, None, cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _state: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Metrics {
        let font = Font { family: self.family.clone(), ..Font::default() };
        let text = cx.text_system();
        let font_id = text.resolve_font(&font);
        // The advance of `M`, as the page measures: one cell is one advance,
        // and the row height is the type's own line height, not the font's.
        let advance = text
            .advance(font_id, px(FONT_SIZE), 'M')
            .map_or(px(FONT_SIZE * 0.6), |s| s.width);
        let metrics = Metrics { font, cell_w: advance, cell_h: px(LINE_HEIGHT) };
        let (rows, cols) = grid_for(
            f32::from(bounds.size.width),
            f32::from(bounds.size.height),
            f32::from(metrics.cell_w),
            f32::from(metrics.cell_h),
        );
        self.reported.set((rows, cols));
        let _ = window;
        metrics
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _state: &mut (),
        metrics: &mut Metrics,
        window: &mut Window,
        cx: &mut App,
    ) {
        let started = Instant::now();
        let palette = &*self.palette;
        window.paint_quad(fill(bounds, palette.bg.hsla()));

        let laid = layout(&self.screen, palette);
        let layout_done = Instant::now();

        let (cell_w, cell_h) = (metrics.cell_w, metrics.cell_h);
        let origin = bounds.origin;
        let at = |row: u16, col: u16| point(origin.x + cell_w * f32::from(col), origin.y + cell_h * f32::from(row));
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            for f in &laid.fills {
                let rect = Bounds {
                    origin: at(f.row, f.col),
                    size: size(cell_w * f32::from(f.cols), cell_h * f32::from(f.rows)),
                };
                window.paint_quad(fill(rect, f.color.hsla()));
            }
            for batch in &laid.batches {
                let mut font = metrics.font.clone();
                if batch.style.bold {
                    font.weight = FontWeight::BOLD;
                }
                if batch.style.italic {
                    font.style = FontStyle::Italic;
                }
                let color = batch.style.fg.hsla();
                let run = TextRun {
                    len: batch.text.len(),
                    font,
                    color,
                    background_color: None,
                    underline: batch.style.underline.then(|| UnderlineStyle {
                        thickness: px(1.0),
                        color: Some(color),
                        wavy: false,
                    }),
                    strikethrough: None,
                };
                // Forcing the advance is what keeps a proportional fallback
                // glyph on the grid; a wide glyph is forced to two cells.
                let force = cell_w * if batch.wide { 2.0 } else { 1.0 };
                let line = window.text_system().shape_line(
                    SharedString::from(batch.text.clone()),
                    px(FONT_SIZE),
                    &[run],
                    Some(force),
                );
                line.paint(at(batch.row, batch.col), cell_h, TextAlign::Left, None, window, cx).ok();
            }
            self.paint_cursor(metrics, origin, window, cx);
        });

        let done = Instant::now();
        let mut stats = self.stats.borrow_mut();
        let spent = done - started;
        stats.frames += 1;
        stats.total += spent;
        stats.worst = stats.worst.max(spent);
        stats.layout_total += layout_done - started;
        if let Some(last) = stats.last_start {
            stats.worst_gap = stats.worst_gap.max(started - last);
        }
        stats.last_start = Some(started);
        stats.quads = laid.fills.len();
        stats.batches = laid.batches.len();
    }
}

impl TerminalElement {
    /// The cursor as the page draws it: a block in the text colour with its
    /// glyph redrawn in the background colour, hollow when the pane has no
    /// keys, or a bar or underline when the child asked for one.
    fn paint_cursor(&self, metrics: &Metrics, origin: gpui_kit::Point<Pixels>, window: &mut Window, cx: &mut App) {
        let cursor = self.screen.cursor;
        if !cursor.visible || cursor.row >= self.screen.rows || cursor.col >= self.screen.cols {
            return;
        }
        let (cell_w, cell_h) = (metrics.cell_w, metrics.cell_h);
        let top_left = point(
            origin.x + cell_w * f32::from(cursor.col),
            origin.y + cell_h * f32::from(cursor.row),
        );
        let color = self.palette.fg.hsla();
        match cursor.shape {
            CursorShape::Bar => {
                window.paint_quad(fill(Bounds { origin: top_left, size: size(px(2.0), cell_h) }, color));
            }
            CursorShape::Underline => {
                let origin = point(top_left.x, top_left.y + cell_h - px(2.0));
                window.paint_quad(fill(Bounds { origin, size: size(cell_w, px(2.0)) }, color));
            }
            CursorShape::Block if !self.focused => {
                let cell = Bounds { origin: top_left, size: size(cell_w, cell_h) };
                window.paint_quad(outline(cell, color, BorderStyle::Solid));
            }
            CursorShape::Block => {
                let wide = self.screen.glyph_at(cursor.row, cursor.col);
                let cells = if wide.chars().next().is_some_and(is_wide) { 2.0 } else { 1.0 };
                window.paint_quad(fill(Bounds { origin: top_left, size: size(cell_w * cells, cell_h) }, color));
                let glyph = self.screen.glyph_at(cursor.row, cursor.col);
                if glyph != " " {
                    let font = Font { family: self.family.clone(), ..Font::default() };
                    let run = TextRun {
                        len: glyph.len(),
                        font,
                        color: self.palette.bg.hsla(),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    };
                    let line = window.text_system().shape_line(
                        SharedString::from(glyph.to_owned()),
                        px(FONT_SIZE),
                        &[run],
                        Some(cell_w * cells),
                    );
                    line.paint(top_left, cell_h, TextAlign::Left, None, window, cx).ok();
                }
            }
        }
    }
}

impl IntoElement for TerminalElement {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

// ---------------------------------------------------------------------------
// Demo content: realistic coloured output
// ---------------------------------------------------------------------------

/// East Asian wide and emoji ranges: enough for the demo and the tests, where
/// the real daemon sends the width it measured.
fn is_wide(c: char) -> bool {
    matches!(u32::from(c),
        0x1100..=0x115f | 0x2e80..=0xa4cf | 0xac00..=0xd7a3 | 0xf900..=0xfaff
        | 0xfe30..=0xfe6f | 0xff00..=0xff60 | 0xffe0..=0xffe6
        | 0x1f300..=0x1f64f | 0x1f900..=0x1f9ff)
}

const fn ansi(i: u32) -> u32 {
    1 << 24 | i
}

const fn truecolor(r: u8, g: u8, b: u8) -> u32 {
    2 << 24 | (r as u32) << 16 | (g as u32) << 8 | b as u32
}

/// One row being built the way the daemon builds it: a narrow cell joins the
/// segment before it, a wide one is sent alone.
struct RowBuilder {
    col: u16,
    segments: Vec<Segment>,
}

impl RowBuilder {
    fn new() -> Self {
        Self { col: 0, segments: Vec::new() }
    }

    fn text(mut self, text: &str, fg: u32, bg: u32, flags: u8) -> Self {
        for ch in text.chars() {
            if is_wide(ch) {
                self.segments.push(Segment(self.col, vec![ch.to_string()], fg, bg, flags | WIDE));
                self.col += 2;
                continue;
            }
            match self.segments.last_mut() {
                Some(last)
                    if last.2 == fg && last.3 == bg && last.4 == flags && last.0 + last.width() == self.col =>
                {
                    last.1.push(ch.to_string());
                }
                _ => self.segments.push(Segment(self.col, vec![ch.to_string()], fg, bg, flags)),
            }
            self.col += 1;
        }
        self
    }

    fn plain(self, text: &str) -> Self {
        self.text(text, 0, 0, 0)
    }

    fn done(self) -> Vec<Segment> {
        self.segments
    }
}

const CRATES: [&str; 12] = [
    "proc-macro2", "quote", "syn", "serde_derive", "serde", "tokio", "bytes", "futures-core",
    "gpui-pre", "gpui-kit", "argus-protocol", "argusd",
];

/// One line of cargo-like output for slot `n`, which scrolls with the tick.
fn log_line(n: usize) -> Vec<Segment> {
    let name = CRATES[n % CRATES.len()];
    let row = RowBuilder::new();
    match n % 9 {
        0..=3 => row
            .text(&format!("{:>12}", "Compiling"), ansi(2), 0, BOLD)
            .plain(&format!(" {name} v0.{}.{}", n % 7, n % 13))
            .text(&format!(" (/home/dev/.cargo/registry/src/{name}-0.{}.{})", n % 7, n % 13), ansi(8), 0, 0)
            .done(),
        4 => row
            .text("warning", ansi(3), 0, BOLD)
            .text(": unused import: ", 0, 0, BOLD)
            .text(&format!("`{name}::prelude::*`"), 0, 0, BOLD | UNDERLINE)
            .done(),
        5 => row
            .text("   --> ", ansi(4), 0, BOLD)
            .plain(&format!("crates/{name}/src/lib.rs:{}:{}", 10 + n % 200, 5 + n % 30))
            .done(),
        6 => row
            .text("-    let value = old_call(&state, 42);", ansi(1), truecolor(0x3a, 0x14, 0x14), 0)
            .done(),
        7 => row
            .text("+    let value = new_call(&state, 42)?;", ansi(2), truecolor(0x10, 0x32, 0x20), 0)
            .done(),
        _ => row
            .text("error[E0308]", ansi(1), 0, BOLD)
            .text(": mismatched types, ", 0, 0, BOLD)
            .text("expected `u16`, found `usize`", 0, 0, ITALIC)
            .done(),
    }
}

/// The 140x40 frame for `tick`: scrolling build output above a fixed sampler
/// of every style the page draws, with a progress bar, a spinner and a
/// gradient that move so no two frames are alike.
fn demo_frame(tick: u32) -> Frame {
    const ROWS: u16 = 40;
    const COLS: u16 = 140;
    const LOG_ROWS: usize = 28;
    let mut lines: Vec<(u16, Vec<Segment>)> = Vec::with_capacity(ROWS as usize);

    let scroll = tick as usize / 3;
    for r in 0..LOG_ROWS {
        lines.push((r as u16, log_line(r + scroll)));
    }

    let spinner = ["|", "/", "-", "\\"][tick as usize / 2 % 4];
    let filled = (tick as usize * 2) % 100;
    let bar = RowBuilder::new()
        .text(&format!("{spinner} "), ansi(6), 0, BOLD)
        .text("Building ", 0, 0, BOLD)
        .text(&" ".repeat(filled / 2), 0, ansi(2), 0)
        .text(&" ".repeat(50 - filled / 2), 0, ansi(8), 0)
        .plain(&format!(" {filled:>3}%"))
        .done();
    lines.push((LOG_ROWS as u16, bar));

    // Every style at once, and the same text on every background.
    let styles = RowBuilder::new()
        .text("bold ", 0, 0, BOLD)
        .text("italic ", 0, 0, ITALIC)
        .text("underline ", 0, 0, UNDERLINE)
        .text("reverse ", 0, 0, REVERSE)
        .text("bold-italic-underline ", ansi(5), 0, BOLD | ITALIC | UNDERLINE)
        .text(" selected ", ansi(7), ansi(4), 0)
        .text(" dark on dark ", ansi(0), ansi(8), 0)
        .done();
    lines.push((LOG_ROWS as u16 + 1, styles));

    let wide = RowBuilder::new()
        .plain("wide cells: ")
        .text("日本語のテキスト", ansi(3), 0, 0)
        .plain(" mixed with ascii, ")
        .text("終端", ansi(6), ansi(0), BOLD)
        .plain(" and an emoji ")
        .text("🚀", 0, 0, 0)
        .plain(" ends the row")
        .done();
    lines.push((LOG_ROWS as u16 + 2, wide));

    let mut ansi_bar = RowBuilder::new();
    for i in 0..16 {
        ansi_bar = ansi_bar.text("   ", 0, ansi(i), 0);
    }
    lines.push((LOG_ROWS as u16 + 3, ansi_bar.done()));

    // The 256-colour cube: worst case for background merging after the gradient.
    for (k, base) in [16u32, 52, 88, 124, 160, 196].into_iter().enumerate() {
        let mut cube = RowBuilder::new();
        for i in 0..36 {
            cube = cube.text("  ", 0, ansi(base + i), 0);
        }
        lines.push((LOG_ROWS as u16 + 4 + k as u16, cube.done()));
    }

    // One background per cell, shifting each tick: nothing for the merge to fold.
    let mut gradient = RowBuilder::new();
    for c in 0..COLS {
        let phase = (u32::from(c) * 255 / u32::from(COLS) + tick * 4) % 256;
        gradient = gradient.text(
            "▀",
            truecolor(255 - phase as u8, phase as u8, 128),
            truecolor(phase as u8, 128, 255 - phase as u8),
            0,
        );
    }
    lines.push((ROWS - 1 - 1, gradient.done()));

    let prompt = RowBuilder::new()
        .text("argus", ansi(4), 0, BOLD)
        .plain(" ")
        .text("~/orion", ansi(6), 0, 0)
        .plain(" ")
        .text("(gpui-migration)", ansi(5), 0, 0)
        .text(" $ ", ansi(2), 0, BOLD)
        .plain("cargo build")
        .done();
    let prompt_cols = 5 + 1 + 7 + 1 + 16 + 3 + 11;
    lines.push((ROWS - 1, prompt));

    Frame {
        rows: ROWS,
        cols: COLS,
        full: true,
        cursor: FrameCursor { row: ROWS - 1, col: prompt_cols, visible: true, shape: [CursorShape::Block, CursorShape::Bar, CursorShape::Underline][tick as usize / 60 % 3] },
        lines,
    }
}

// ---------------------------------------------------------------------------
// The window
// ---------------------------------------------------------------------------

struct TerminalStage {
    screen: Arc<Screen>,
    palette: Rc<Palette>,
    reported: Rc<Cell<(u16, u16)>>,
    stats: Rc<RefCell<PaintStats>>,
    /// Held so the repaint loop runs as long as the stage does.
    _driver: Task<()>,
}

impl TerminalStage {
    fn new(seconds: u64, cx: &mut Context<Self>) -> Self {
        let reported = Rc::new(Cell::new((0, 0)));
        let stats = Rc::new(RefCell::new(PaintStats::default()));
        let (report_for_driver, stats_for_driver) = (reported.clone(), stats.clone());
        let driver = cx.spawn(async move |this, cx| {
            let begun = Instant::now();
            let mut tick = 0u32;
            while begun.elapsed() < Duration::from_secs(seconds) {
                cx.background_executor().timer(FRAME_INTERVAL).await;
                tick += 1;
                let alive = this.update(cx, |stage, cx| {
                    let mut screen = Screen::new();
                    screen.apply(demo_frame(tick));
                    stage.screen = Arc::new(screen);
                    cx.notify();
                });
                if alive.is_err() {
                    return;
                }
            }
            print_report(&stats_for_driver.borrow(), report_for_driver.get(), tick);
            cx.update(|cx| cx.quit());
        });
        let mut screen = Screen::new();
        screen.apply(demo_frame(0));
        Self {
            screen: Arc::new(screen),
            palette: Rc::new(Palette::ledger()),
            reported,
            stats,
            _driver: driver,
        }
    }
}

fn print_report(stats: &PaintStats, grid: (u16, u16), ticks: u32) {
    let frames = stats.frames.max(1);
    let avg = stats.total / frames;
    println!("terminal spike: {ticks} frames sent, {} painted", stats.frames);
    println!("  element reported {} rows x {} cols for its size", grid.0, grid.1);
    println!("  paint (layout + record, CPU side): avg {avg:?}, worst {:?}", stats.worst);
    println!("    of which row layout: avg {:?}", stats.layout_total / frames);
    println!("  last frame: {} background quads, {} text runs", stats.quads, stats.batches);
    println!("  longest gap between paints: {:?}", stats.worst_gap);
}

impl Render for TerminalStage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ledger = *Ledger::global(cx);
        let family = cx.theme().mono_font_family.clone();
        div()
            .size_full()
            .bg(ledger.background)
            .p(px(8.0))
            .child(TerminalElement {
                screen: self.screen.clone(),
                palette: self.palette.clone(),
                family,
                focused: true,
                reported: self.reported.clone(),
                stats: self.stats.clone(),
            })
    }
}

fn seconds_from_args() -> u64 {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--seconds" {
            return args.next().and_then(|n| n.parse().ok()).unwrap_or(DEFAULT_SECONDS);
        }
    }
    DEFAULT_SECONDS
}

fn main() {
    let seconds = seconds_from_args();
    window::application().run(move |cx: &mut App| {
        window::init(cx);
        window::open(cx, |_, cx| cx.new(|cx| TerminalStage::new(seconds, cx)))
            .expect("the terminal window opens");
        cx.activate(true);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(segments: Vec<Segment>) -> Screen {
        Screen {
            rows: 1,
            cols: 80,
            lines: vec![segments],
            cursor: FrameCursor { row: 0, col: 0, visible: false, shape: CursorShape::Block },
        }
    }

    fn rows(lines: Vec<Vec<Segment>>) -> Screen {
        Screen { rows: lines.len() as u16, cols: 80, lines, ..row(Vec::new()) }
    }

    fn seg(col: u16, text: &str, fg: u32, bg: u32, flags: u8) -> Segment {
        Segment(col, text.chars().map(|c| c.to_string()).collect(), fg, bg, flags)
    }

    #[test]
    fn touching_segments_of_one_style_are_one_run() {
        let screen = row(vec![seg(0, "ab", 0, 0, 0), seg(2, "cd", 0, 0, 0)]);
        let laid = layout(&screen, &Palette::ledger());
        assert_eq!(laid.batches.len(), 1);
        assert_eq!(laid.batches[0].text, "abcd");
        assert_eq!(laid.batches[0].cols, 4);
    }

    #[test]
    fn a_change_of_colour_or_weight_starts_a_new_run() {
        let screen = row(vec![
            seg(0, "ab", 0, 0, 0),
            seg(2, "cd", ansi(1), 0, 0),
            seg(4, "ef", ansi(1), 0, BOLD),
        ]);
        let laid = layout(&screen, &Palette::ledger());
        let texts: Vec<_> = laid.batches.iter().map(|b| b.text.as_str()).collect();
        assert_eq!(texts, ["ab", "cd", "ef"]);
        assert!(laid.batches[2].style.bold);
    }

    #[test]
    fn a_gap_between_segments_splits_a_run() {
        let screen = row(vec![seg(0, "ab", 0, 0, 0), seg(5, "cd", 0, 0, 0)]);
        let laid = layout(&screen, &Palette::ledger());
        assert_eq!(laid.batches.len(), 2);
        assert_eq!(laid.batches[1].col, 5);
    }

    #[test]
    fn a_background_does_not_split_a_run_of_one_text_colour() {
        let screen = row(vec![seg(0, "ab", ansi(0), 0, 0), seg(2, "cd", ansi(0), ansi(4), 0)]);
        let laid = layout(&screen, &Palette::ledger());
        assert_eq!(laid.batches.len(), 1);
        assert_eq!(laid.fills.len(), 1);
    }

    #[test]
    fn blank_runs_paint_nothing_unless_underlined() {
        let screen = row(vec![seg(0, "   ", 0, 0, 0), seg(5, "  ", 0, 0, UNDERLINE)]);
        let laid = layout(&screen, &Palette::ledger());
        assert_eq!(laid.batches.len(), 1);
        assert_eq!(laid.batches[0].col, 5);
    }

    #[test]
    fn a_wide_cell_takes_two_columns_and_the_next_segment_follows_it() {
        let built = RowBuilder::new().plain("a").text("日", 0, 0, 0).plain("b").done();
        assert_eq!(built[1].0, 1);
        assert_eq!(built[1].width(), 2);
        assert_eq!(built[2].0, 3, "the spacer column is not sent but is not free");
        let laid = layout(&row(built), &Palette::ledger());
        let wide: Vec<_> = laid.batches.iter().filter(|b| b.wide).collect();
        assert_eq!(wide.len(), 1);
        assert_eq!((wide[0].col, wide[0].cols), (1, 2));
        assert_eq!(laid.batches.last().map(|b| b.col), Some(3));
    }

    #[test]
    fn a_wide_cell_is_never_merged_into_its_neighbours() {
        let built = RowBuilder::new().plain("ab").text("日本", 0, 0, 0).plain("cd").done();
        let laid = layout(&row(built), &Palette::ledger());
        let cols: Vec<_> = laid.batches.iter().map(|b| (b.col, b.cols, b.wide)).collect();
        assert_eq!(cols, [(0, 2, false), (2, 2, true), (4, 2, true), (6, 2, false)]);
    }

    #[test]
    fn a_wide_cell_background_covers_both_columns() {
        let built = RowBuilder::new().text("日", 0, ansi(1), 0).done();
        let laid = layout(&row(built), &Palette::ledger());
        assert_eq!(laid.fills, [Fill { row: 0, col: 0, cols: 2, rows: 1, color: Palette::ledger().ansi[1] }]);
    }

    #[test]
    fn touching_backgrounds_of_one_colour_merge_along_a_row() {
        let screen = row(vec![
            seg(0, "ab", 0, ansi(4), 0),
            seg(2, "cd", ansi(2), ansi(4), BOLD),
            seg(4, "ef", 0, ansi(1), 0),
            seg(8, "gh", 0, ansi(1), 0),
        ]);
        let laid = layout(&screen, &Palette::ledger());
        let spans: Vec<_> = laid.fills.iter().map(|f| (f.col, f.cols)).collect();
        assert_eq!(spans, [(0, 4), (4, 2), (8, 2)]);
    }

    #[test]
    fn identical_rectangles_on_consecutive_rows_merge_down() {
        let block = |text| vec![seg(2, text, 0, ansi(1), 0)];
        let screen = rows(vec![block("xxxx"), block("yyyy"), block("zzzz"), block("xxx")]);
        let laid = layout(&screen, &Palette::ledger());
        let rects: Vec<_> = laid.fills.iter().map(|f| (f.row, f.cols, f.rows)).collect();
        assert_eq!(rects, [(0, 4, 3), (3, 3, 1)]);
    }

    #[test]
    fn default_background_cells_make_no_quad() {
        let laid = layout(&row(vec![seg(0, "abc", ansi(2), 0, 0)]), &Palette::ledger());
        assert!(laid.fills.is_empty());
    }

    #[test]
    fn reverse_swaps_text_and_background_and_defaults_to_the_theme() {
        let palette = Palette::ledger();
        let laid = layout(&row(vec![seg(0, "ab", 0, 0, REVERSE)]), &palette);
        assert_eq!(laid.fills[0].color, palette.fg);
        assert_eq!(laid.batches[0].style.fg, palette.bg);
    }

    #[test]
    fn text_the_background_would_swallow_is_moved_to_the_contrast_floor() {
        let palette = Palette::ledger();
        // The theme's dark grey on its own dark grey fill.
        let (fore, back) = segment_colours(&seg(0, "x", ansi(0), ansi(8), 0), &palette);
        let back = back.expect("a background was set");
        assert!(contrast(fore, back) >= MINIMUM_CONTRAST);
        assert_ne!(fore, palette.ansi[0]);
        // Readable text is left alone.
        let (fore, _) = segment_colours(&seg(0, "x", ansi(15), ansi(0), 0), &palette);
        assert_eq!(fore, palette.ansi[15]);
    }

    #[test]
    fn packed_colours_resolve_as_the_page_resolves_them() {
        let palette = Palette::ledger();
        assert_eq!(resolve(0, &palette), None);
        assert_eq!(resolve(ansi(1), &palette), Some(palette.ansi[1]));
        assert_eq!(resolve(truecolor(1, 2, 3), &palette), Some(Rgb(1, 2, 3)));
        assert_eq!(resolve(ansi(16), &palette), Some(Rgb(0, 0, 0)));
        assert_eq!(resolve(ansi(231), &palette), Some(Rgb(255, 255, 255)));
        assert_eq!(resolve(ansi(232), &palette), Some(Rgb(8, 8, 8)));
        assert_eq!(resolve(ansi(255), &palette), Some(Rgb(238, 238, 238)));
    }

    #[test]
    fn rows_and_columns_are_what_whole_cells_fit() {
        assert_eq!(grid_for(1120.0, 800.0, 8.0, 20.0), (40, 140));
        assert_eq!(grid_for(1127.0, 819.0, 8.0, 20.0), (40, 140));
        assert_eq!(grid_for(1119.9, 799.0, 8.0, 20.0), (39, 139));
    }

    #[test]
    fn a_width_a_hair_under_a_whole_number_of_cells_still_counts_it() {
        let cell = 7.8_f32;
        let width = cell * 140.0 - 0.0001;
        assert_eq!(grid_for(width, 20.0, cell, 20.0).1, 140);
    }

    #[test]
    fn a_pane_with_no_room_is_one_cell_never_zero() {
        assert_eq!(grid_for(0.0, 0.0, 8.0, 20.0), (1, 1));
        assert_eq!(grid_for(3.0, 5.0, 8.0, 20.0), (1, 1));
    }

    #[test]
    fn a_partial_frame_replaces_only_its_rows_and_a_full_one_clears_the_rest() {
        let cursor = FrameCursor { row: 0, col: 0, visible: true, shape: CursorShape::Bar };
        let mut screen = Screen::new();
        screen.apply(Frame {
            rows: 2,
            cols: 10,
            full: true,
            cursor,
            lines: vec![(0, vec![seg(0, "one", 0, 0, 0)]), (1, vec![seg(0, "two", 0, 0, 0)])],
        });
        screen.apply(Frame { rows: 2, cols: 10, full: false, cursor, lines: vec![(1, vec![seg(0, "new", 0, 0, 0)])] });
        assert_eq!(screen.glyph_at(0, 0), "o");
        assert_eq!(screen.glyph_at(1, 0), "n");
        screen.apply(Frame { rows: 2, cols: 10, full: true, cursor, lines: vec![(1, vec![seg(0, "x", 0, 0, 0)])] });
        assert_eq!(screen.glyph_at(0, 0), " ");
        assert_eq!(screen.glyph_at(1, 0), "x");
    }

    #[test]
    fn the_cursor_reads_the_glyph_under_a_wide_cell_from_either_column() {
        let built = RowBuilder::new().plain("a").text("日", 0, 0, 0).done();
        let screen = row(built);
        assert_eq!(screen.glyph_at(0, 1), "日");
        assert_eq!(screen.glyph_at(0, 2), "日");
        assert_eq!(screen.glyph_at(0, 3), " ");
    }

    #[test]
    fn the_demo_frame_is_a_full_140_by_40_screen_the_layout_folds() {
        let frame = demo_frame(7);
        assert_eq!((frame.rows, frame.cols, frame.full), (40, 140, true));
        assert!(frame.lines.iter().all(|(r, _)| *r < 40));
        let mut screen = Screen::new();
        screen.apply(frame);
        let laid = layout(&screen, &Palette::ledger());
        let cells_with_background: usize = screen
            .lines
            .iter()
            .flatten()
            .filter(|s| s.3 != 0 || s.4 & REVERSE != 0)
            .map(|s| s.width() as usize)
            .sum();
        let merged: usize = laid.fills.iter().map(|f| (f.cols * f.rows) as usize).sum();
        assert_eq!(merged, cells_with_background, "merging loses no cell");
        assert!(laid.fills.len() < cells_with_background, "the diff blocks and bars fold");
    }
}
