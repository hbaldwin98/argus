// A pane's screen in its container: the rows Rust sends, drawn in the
// theme's colours on canvases of its own, and the room the container has
// reported back as rows and columns. There is one screen per side Rust
// draws on: the Workspace's, the console's first and second, and one for
// each extension console view on screen, so an agent and a docked console
// are drawn at once.
// The rows are mirrored only so a theme or scale change can repaint them.

import type { Frame } from './bindings/Frame';
import type { FrameCursor } from './bindings/FrameCursor';
import type { Parked } from './bindings/Parked';
import type { Segment } from './bindings/Segment';
import type { Side } from './bindings/Side';
import { $, el, ghosts } from './kit/dom.js';
import { dragging } from './edge.js';
import { call } from './tauri.js';

// The terminal's type, as the design system's --type-mono gives it.
const FONT_SIZE = 13;
const LINE_HEIGHT = 20;

// A segment's flags, as `src/page.rs` packs them.
const BOLD = 1;
const ITALIC = 2;
const UNDERLINE = 4;
const REVERSE = 8;
const WIDE = 16;

type Cell = [number, number];

interface Palette {
  fg: string;
  bg: string;
  ansi: string[];
}

// One level of the 256-colour cube.
function step(c: number): number {
  return c ? 55 + 40 * c : 0;
}

function clamp(n: number, most: number): number {
  return Math.max(0, Math.min(most - 1, n));
}

function canvas(): [HTMLCanvasElement, CanvasRenderingContext2D] {
  const node = el('canvas', { 'aria-hidden': 'true' });
  const context = node.getContext('2d');
  if (!context) throw new Error('a canvas that draws nothing');
  return [node, context];
}

function chosenFamily(): string {
  const css = getComputedStyle(document.documentElement);
  return css.getPropertyValue('--font-mono').trim() || 'monospace';
}

// `side` is the screen Rust draws on; an extension console view's has none
// until the window gives it one, and is given another if it comes back.
function makeScreen(terminal: HTMLElement, first: Side | null) {
  let side = first;
  const [text, tctx] = canvas();
  const [cursorCanvas, cctx] = canvas();
  // Bars stand in for a pane until its first screen arrives.
  const waiting = el('div', { class: 'term-skeleton', hidden: true });
  waiting.replaceChildren(ghosts([[0.55], [0.8], [0.7], [0.85], [0.4], [0.75], [0.6]]));
  terminal.prepend(waiting, text, cursorCanvas);

  let pane: number | null = null;
  let rows = 0;
  let cols = 0;
  let mirror: Segment[][] = [];
  let cursor: FrameCursor = { row: 0, col: 0, visible: false, shape: 'block' };
  let cell = { w: 8, h: LINE_HEIGHT, ascent: 10, dpr: 1 };
  let palette: Palette = { fg: '', bg: '', ansi: [] };
  let fontFamily = 'monospace';
  // A drag across the pane, `[row, col]` at each end. Only drawn here; the
  // text it covers is read from Rust's grid when the drag ends.
  let selection: { anchor: Cell; head: Cell } | null = null;
  let parked: Parked | null = null;
  let mouse = false;
  // Whether this side has the keys: the other side of a split draws its
  // cursor hollow, as an unfocused window does.
  let keyed = true;

  function measure(): void {
    fontFamily = chosenFamily();
    tctx.font = `${FONT_SIZE}px ${fontFamily}`;
    const m = tctx.measureText('M'.repeat(100));
    const ascent = m.fontBoundingBoxAscent || FONT_SIZE * 0.8;
    const descent = m.fontBoundingBoxDescent || FONT_SIZE * 0.2;
    cell = {
      w: m.width / 100,
      h: LINE_HEIGHT,
      ascent: (LINE_HEIGHT - (ascent + descent)) / 2 + ascent,
      dpr: window.devicePixelRatio || 1,
    };
  }

  function readPalette(): void {
    const css = getComputedStyle(document.documentElement);
    const v = (name: string): string => css.getPropertyValue(name).trim();
    palette = { fg: v('--fg'), bg: v('--bg-sunken'), ansi: [] };
    for (let i = 0; i < 16; i++) palette.ansi.push(v(`--ansi-${i}`));
  }

  // Packed as on the wire: 0 the default, 1 << 24 | index, 2 << 24 | rgb.
  function colour(packed: number, fallback: string | null): string | null {
    const kind = packed >>> 24;
    if (kind === 0) return fallback;
    if (kind === 2) return `rgb(${(packed >> 16) & 255},${(packed >> 8) & 255},${packed & 255})`;
    const i = packed & 255;
    if (i < 16) return palette.ansi[i] ?? fallback;
    if (i < 232) {
      const n = i - 16;
      return `rgb(${step(Math.floor(n / 36))},${step(Math.floor(n / 6) % 6)},${step(n % 6)})`;
    }
    const grey = 8 + 10 * (i - 232);
    return `rgb(${grey},${grey},${grey})`;
  }

  function size(): void {
    const w = Math.ceil(cols * cell.w);
    const h = rows * cell.h;
    for (const node of [text, cursorCanvas]) {
      node.width = Math.ceil(w * cell.dpr);
      node.height = Math.ceil(h * cell.dpr);
      node.style.width = `${w}px`;
      node.style.height = `${h}px`;
    }
    tctx.setTransform(cell.dpr, 0, 0, cell.dpr, 0, 0);
    cctx.setTransform(cell.dpr, 0, 0, cell.dpr, 0, 0);
    tctx.textBaseline = 'alphabetic';
  }

  function drawRow(row: number): void {
    const y = row * cell.h;
    tctx.clearRect(0, y, cols * cell.w + 1, cell.h);
    for (const [col, cells, fg, bg, flags] of mirror[row] ?? []) {
      let fore = colour(fg, palette.fg) ?? palette.fg;
      let back = colour(bg, null);
      if (flags & REVERSE) [fore, back] = [back ?? palette.bg, fore];
      const x = col * cell.w;
      const width = (flags & WIDE ? 2 : cells.length) * cell.w;
      if (back) {
        tctx.fillStyle = back;
        tctx.fillRect(x, y, width, cell.h);
      }
      const italic = flags & ITALIC ? 'italic ' : '';
      const bold = flags & BOLD ? '700 ' : '';
      tctx.font = `${italic}${bold}${FONT_SIZE}px ${fontFamily}`;
      tctx.fillStyle = fore;
      if (flags & WIDE) {
        const glyph = cells[0] ?? '';
        const drawn = tctx.measureText(glyph).width;
        tctx.fillText(glyph, x + (2 * cell.w - drawn) / 2, y + cell.ascent);
      } else {
        cells.forEach((ch, i) => {
          if (ch !== ' ') tctx.fillText(ch, x + i * cell.w, y + cell.ascent);
        });
      }
      if (flags & UNDERLINE) tctx.fillRect(x, y + cell.h - 3, width, 1);
    }
  }

  function glyphAt(row: number, col: number): string {
    for (const [start, cells, , , flags] of mirror[row] ?? []) {
      const span = flags & WIDE ? 2 : cells.length;
      if (col >= start && col < start + span) {
        return (flags & WIDE ? cells[0] : cells[col - start]) ?? ' ';
      }
    }
    return ' ';
  }

  function drawSelection(): void {
    if (!selection) return;
    const [start, end] =
      selection.anchor[0] < selection.head[0] ||
      (selection.anchor[0] === selection.head[0] && selection.anchor[1] <= selection.head[1])
        ? [selection.anchor, selection.head]
        : [selection.head, selection.anchor];
    cctx.save();
    cctx.globalAlpha = 0.3;
    cctx.fillStyle = palette.fg;
    for (let row = start[0]; row <= end[0] && row < rows; row++) {
      const from = row === start[0] ? start[1] : 0;
      const to = row === end[0] ? end[1] + 1 : cols;
      cctx.fillRect(from * cell.w, row * cell.h, (to - from) * cell.w, cell.h);
    }
    cctx.restore();
  }

  function drawCursor(): void {
    cctx.clearRect(0, 0, cursorCanvas.width, cursorCanvas.height);
    drawSelection();
    if (!cursor.visible || cursor.row >= rows || cursor.col >= cols) return;
    const x = cursor.col * cell.w;
    const y = cursor.row * cell.h;
    const focused = keyed && document.activeElement === $('input') && document.hasFocus();
    cctx.fillStyle = palette.fg;
    cctx.strokeStyle = palette.fg;
    if (cursor.shape === 'bar') {
      cctx.fillRect(x, y, 2, cell.h);
    } else if (cursor.shape === 'underline') {
      cctx.fillRect(x, y + cell.h - 2, cell.w, 2);
    } else if (!focused) {
      cctx.strokeRect(x + 0.5, y + 0.5, cell.w - 1, cell.h - 1);
    } else {
      cctx.fillRect(x, y, cell.w, cell.h);
      cctx.font = `${FONT_SIZE}px ${fontFamily}`;
      cctx.fillStyle = palette.bg;
      cctx.fillText(glyphAt(cursor.row, cursor.col), x, y + cell.ascent);
    }
  }

  function repaint(): void {
    size();
    for (let row = 0; row < rows; row++) drawRow(row);
    drawCursor();
  }

  function expect(next: number | null): void {
    waiting.hidden = next === null || next === pane;
  }

  function frame(f: Frame): void {
    waiting.hidden = true;
    // The page can run before the stylesheet applies, when every token
    // reads empty; a colour the canvas cannot parse leaves the last one
    // standing, so the palette is read again until it is there.
    if (!palette.fg) readPalette();
    if (f.pane !== pane || f.full) {
      if (f.pane !== pane) selection = null;
      pane = f.pane;
      mirror = Array.from({ length: f.rows }, (): Segment[] => []);
    }
    const resized = f.rows !== rows || f.cols !== cols;
    const moved = f.parked?.offset !== parked?.offset || f.parked?.depth !== parked?.depth;
    rows = f.rows;
    cols = f.cols;
    parked = f.parked;
    mouse = f.mouse;
    if (resized) {
      mirror.length = rows;
      for (let row = 0; row < rows; row++) mirror[row] ??= [];
      size();
    }
    // The Workspace's header names its pane's size.
    if ((resized || moved) && side === 'workspace') {
      const label = document.getElementById('size');
      if (label) label.textContent = sizeLabel();
    }
    for (const [row, segments] of f.lines) mirror[row] = segments;
    if (resized) {
      for (let row = 0; row < rows; row++) drawRow(row);
    } else {
      for (const [row] of f.lines) drawRow(row);
    }
    cursor = f.cursor;
    drawCursor();
  }

  // The cell under a point, held inside the grid so a drag past its edge
  // selects to the edge.
  function cellAt(x: number, y: number): Cell {
    const box = text.getBoundingClientRect();
    return [
      clamp(Math.floor((y - box.top) / cell.h), rows),
      clamp(Math.floor((x - box.left) / cell.w), cols),
    ];
  }

  function select(next: { anchor: Cell; head: Cell } | null): void {
    selection = next;
    drawCursor();
  }

  // A parked view leads with how far back it is: otherwise it looks like
  // a pane gone quiet.
  function sizeLabel(): string {
    if (!cols) return '';
    const back = parked ? `↑ ${parked.offset}/${parked.depth} · ` : '';
    return `${back}${cols} × ${rows}`;
  }

  // The room the side has, in cells. The daemon sizes a pane to the
  // client the person was last at, so the grid that comes back may differ.
  let sent: { rows: number; cols: number } | null = null;
  let resizeTimer: ReturnType<typeof setTimeout> | undefined;
  function report(): void {
    clearTimeout(resizeTimer);
    resizeTimer = setTimeout(() => {
      // A hidden terminal, or one inside another stage's hidden body, has
      // no room to report: measuring it would size the pane to one cell,
      // and the pane takes the size of the client last in use. The resize
      // observer reports again once it is laid out.
      if (side === null || terminal.hidden || !terminal.clientWidth || !terminal.clientHeight) {
        return;
      }
      // A column's edge being dragged would reflow the pane at every
      // step; it is measured once the edge is let go.
      if (dragging()) {
        report();
        return;
      }
      const style = getComputedStyle(terminal);
      const w =
        terminal.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight);
      const h =
        terminal.clientHeight - parseFloat(style.paddingTop) - parseFloat(style.paddingBottom);
      const next = {
        rows: Math.max(1, Math.floor(h / cell.h)),
        cols: Math.max(1, Math.floor(w / cell.w)),
      };
      if (sent && sent.rows === next.rows && sent.cols === next.cols) return;
      sent = next;
      call('resize', { side, ...next });
    }, 50);
  }

  // A canvas never asks for a web font, and `load` does not wait for one
  // nothing has used, so the grid would be measured in the fallback face.
  function loadFaces(): void {
    const faces = ['', '700 ', 'italic ', 'italic 700 '];
    const family = chosenFamily();
    void Promise.all(faces.map((face) => document.fonts.load(`${face}${FONT_SIZE}px ${family}`)))
      .catch(() => {})
      .then(rescale);
  }

  function rescale(): void {
    measure();
    readPalette();
    repaint();
    sent = null;
    report();
  }

  // A device-pixel-ratio change (a move to another monitor, a zoom) needs
  // a fresh measurement; matchMedia is how a page hears of one.
  function watchScale(): void {
    const query = matchMedia(`(resolution: ${window.devicePixelRatio}dppx)`);
    query.addEventListener(
      'change',
      () => {
        rescale();
        watchScale();
      },
      { once: true },
    );
  }

  measure();
  readPalette();
  window.addEventListener('load', rescale);
  loadFaces();
  new ResizeObserver(report).observe(terminal);
  // A side appearing after it was hidden needs its room reported.
  new MutationObserver(report).observe(terminal, { attributes: true, attributeFilter: ['hidden'] });
  watchScale();
  matchMedia('(prefers-color-scheme: light)').addEventListener('change', () => {
    readPalette();
    repaint();
  });

  // Whether the child asked for the mouse: a press is then its, and only
  // Shift-drag selects here.
  function takesMouse(): boolean {
    return mouse && !parked;
  }

  /** The theme changed under the canvas: its colours are read again. */
  /** The theme's colours, and the Settings dialog's font, which changes
   *  the cell and so how many fit. */
  function retheme(): void {
    readPalette();
    repaint();
    if (chosenFamily() !== fontFamily) loadFaces();
  }

  /** Draws on `next` from now on, reporting the room it has there; Rust
   *  sends the screen whole once it hears. */
  function bind(next: Side | null): void {
    if (JSON.stringify(next) === JSON.stringify(side)) return;
    side = next;
    sent = null;
    if (next === null) {
      pane = null;
      mirror = [];
      selection = null;
    } else {
      report();
    }
  }

  /** Whether this side has the keys, which its cursor shows. */
  function key(on: boolean): void {
    if (keyed === on) return;
    keyed = on;
    drawCursor();
  }

  return {
    terminal,
    pane: (): number | null => pane,
    side: (): Side | null => side,
    bind,
    frame,
    drawCursor,
    report,
    sizeLabel,
    /** The size of one cell, in CSS pixels. */
    cellSize: (): { w: number; h: number } => ({ w: cell.w, h: cell.h }),
    cellAt,
    select,
    takesMouse,
    expect,
    retheme,
    key,
  };
}

export type Screen = ReturnType<typeof makeScreen>;

/** The Workspace's pane. */
export const screen = makeScreen($('terminal'), 'workspace');
/** The console's first side, the whole of it when it is not split. */
export const consoleScreen = makeScreen($('terminal-console'), 'first');
/** The console's second side, while it is split. */
export const beside = makeScreen($('terminal-beside'), 'second');
/** Every screen, the extension console views' too as they are made. */
export const screens: Screen[] = [screen, consoleScreen, beside];

type Watcher = (s: Screen) => void;
const watchers: Watcher[] = [];

/** Calls `watch` with every screen there is and every one made after. */
export function watchScreens(watch: Watcher): void {
  watchers.push(watch);
  for (const s of screens) watch(s);
}

/** Extension console views' screens, by the number Rust gave the side. */
const views = new Map<number, Screen>();

/** A screen in an extension console view's `terminal`. It draws nowhere
 *  until `bindView` gives it the side Rust drew the view on. */
export function viewScreen(terminal: HTMLElement): Screen {
  const s = makeScreen(terminal, null);
  screens.push(s);
  for (const watch of watchers) watch(s);
  return s;
}

/** The screens of the panes split beside the Workspace's, by the number
 *  Rust gives each. They are kept for good once made, so a pane closed and
 *  another opened reuse a terminal rather than leaking one. */
const besides = new Map<number, Screen>();

/** The screen drawing the `n`th pane beside the Workspace's, made in
 *  `terminal` the first time. */
export function splitScreen(n: number, terminal: HTMLElement): Screen {
  const known = besides.get(n);
  if (known) return known;
  const s = makeScreen(terminal, { beside: n });
  besides.set(n, s);
  screens.push(s);
  for (const watch of watchers) watch(s);
  return s;
}

/** The screen of the `n`th pane beside the Workspace's, once made. */
export function besideScreen(n: number): Screen | null {
  return besides.get(n) ?? null;
}

/** Draws `s` on `side`, or nowhere when its view is not on screen. */
export function bindView(s: Screen, side: Side | null): void {
  for (const [n, v] of views) if (v === s) views.delete(n);
  if (side !== null && typeof side === 'object' && 'view' in side) views.set(side.view, s);
  s.bind(side !== null && typeof side === 'object' ? side : null);
}

/** The extension console view with the keys, which Rust says. */
let keyedView: Screen | null = null;

export function keyView(s: Screen | null): void {
  keyedView = s;
}

export function keyedViewScreen(): Screen | null {
  return keyedView;
}

export function onSide(side: Side): Screen | null {
  if (typeof side === 'object') {
    return ('view' in side ? views.get(side.view) : besides.get(side.beside)) ?? null;
  }
  if (side === 'first') return consoleScreen;
  return side === 'second' ? beside : screen;
}

/** A frame goes to the side Rust says draws it; one for a view the page has
 *  yet to draw is dropped, as Rust sends the screen whole when it is. */
export function drawFrame(f: Frame): void {
  onSide(f.side)?.frame(f);
}
