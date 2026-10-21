// The Workspace's panes side by side: a cell for each, placed by the grid
// Rust sends with the rail, a gutter between each two neighbours to size
// them, and a header on each that can be dragged onto another to move it.
// The arithmetic is `split_math.ts`; this holds the layout's state and
// draws it. Rust keeps the layout and the sizes, and the page only holds a
// size from the moment a gutter is let go until Rust answers with it.
//
// A drag of a gutter moves the cells at most once a frame and sends their
// new sizes once, when it is let go, so no pane is resized while the
// pointer is moving: the panes report their room when `resizing` is
// lifted, each only if its own size changed.

import type { Shown } from '../bindings/Shown';
import type { Zone } from '../bindings/Zone';
import { $, el, icon } from '../kit/dom.js';
import { browserView } from '../browser/pane.js';
import { splitScreen, screen } from './screen.js';
import { state } from '../state.js';
import { call } from '../connection/tauri.js';
import {
  cellRects,
  dragGutter,
  dropZone,
  type Rect,
  staleView,
  toThousandths,
  zoneRect,
} from './split_math.js';

interface Cell {
  box: HTMLElement;
  terminal: HTMLElement;
  head: HTMLElement | null;
  shown: boolean;
  /** The browser view that fills this cell in place of its terminal. */
  view: HTMLElement | null;
}

const cells: Cell[] = [];

/** Sizes in thousandths, row by row: Rust's, or the ones a gutter was let
 *  go at until Rust answers. */
interface Sizes {
  heights: number[];
  widths: number[][];
}
let held: Sizes | null = null;
let heldTimer: ReturnType<typeof setTimeout> | undefined;
/** A gutter or a header is being dragged, so nothing redraws under it. */
let busy = false;

const HEAD_PX = 28;
const MOVE_THRESHOLD = 4;

function splitEl(): HTMLElement {
  return $('split');
}

function sameSizes(a: Sizes, b: Sizes): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

function sizes(): Sizes {
  const { heights, widths } = state.rail.row;
  return held ?? { heights, widths };
}

function ensureCell(i: number): Cell {
  const known = cells[i];
  if (known) return known;
  if (i === 0) {
    const cell: Cell = {
      box: $('gcell-0'),
      terminal: $('terminal'),
      head: null,
      shown: true,
      view: null,
    };
    cells[0] = cell;
    return cell;
  }
  const terminal = el('div', { class: 'terminal' });
  const box = el('div', { class: 'gcell', hidden: true }, terminal);
  splitEl().append(box);
  splitScreen(i - 1, terminal);
  const cell: Cell = { box, terminal, head: null, shown: false, view: null };
  cells[i] = cell;
  return cell;
}

/** Fills `cell` with `browser`'s view, or gives it back to its terminal.
 *  The terminal is only hidden, never taken out, so its screen is there
 *  again when a terminal pane is shown in the cell. */
function showBrowser(cell: Cell, browser: number | null): void {
  const view = browser === null ? null : browserView(browser);
  staleView(cell.view, view, cell.box)?.remove();
  cell.view = view;
  cell.box.classList.toggle('bcell', view !== null);
  if (view && view.parentElement !== cell.box) cell.box.append(view);
}

/** The pane in slot `i`: 0 is the Workspace's own. */
function paneAt(i: number): Shown | null {
  return i === 0 ? state.rail.shown : (state.rail.row.beside[i - 1] ?? null);
}

function percent(n: number): string {
  return `${(n * 100).toFixed(3)}%`;
}

function place(box: HTMLElement, r: Rect): void {
  box.style.left = percent(r.x);
  box.style.top = percent(r.y);
  box.style.width = percent(r.w);
  box.style.height = percent(r.h);
}

/** Each slot's rectangle, from the grid and the sizes. */
function slotRects(of: Sizes): Map<number, Rect> {
  const rects = cellRects(of.heights, of.widths);
  const out = new Map<number, Rect>();
  state.rail.row.grid.forEach((row, r) =>
    row.forEach((slot, c) => {
      const rect = rects[r]?.[c];
      if (rect) out.set(slot, rect);
    }),
  );
  return out;
}

function head(i: number, pane: Shown): HTMLElement {
  const row = state.rail.row;
  const keyed = row.keyed === i;
  return el(
    'div',
    {
      class: `thead gripper${keyed ? ' on' : ''}`,
      title: 'Drag onto another pane to move this one',
      onmousedown: (e: MouseEvent) => startMove(e, i, pane.pane),
    },
    el('span', { class: 'ttl' }, pane.title),
    el('span', { class: `sdot ${pane.status}`, 'aria-hidden': 'true' }),
    // The Workspace's own pane cannot be taken out, but keeps the button's
    // room so the status dots line up across the headers.
    i === 0
      ? el('span', { class: 'x-room', 'aria-hidden': 'true' })
      : el(
          'button',
          {
            class: 'icon x',
            title: 'Take out of the grid',
            'aria-label': `Take ${pane.title} out of the grid`,
            // A press on the close must not start a move.
            onmousedown: (e: MouseEvent) => e.stopPropagation(),
            onclick: () => call('close_beside', { side: { beside: i - 1 } }),
          },
          icon('close'),
        ),
  );
}

/** Draws the cells, their headers and the gutters between them from the
 *  rail. Called with every redraw of the window. */
export function drawSplit(): void {
  const area = splitEl();
  const wanted = state.rail.shown ? 1 + state.rail.row.beside.length : 0;
  if (held && sameSizes(held, { heights: state.rail.row.heights, widths: state.rail.row.widths })) {
    held = null;
  }
  area.hidden = wanted === 0;
  if (busy) return;
  for (let i = 0; i < Math.max(wanted, cells.length); i++) {
    const cell = ensureCell(i);
    const visible = i < wanted;
    cell.box.hidden = !visible;
    if (!visible) showBrowser(cell, null);
    if (visible && !cell.shown && i > 0) {
      // Back on screen after being let go: it has to say its room again,
      // since Rust forgot the size when it dropped the pane.
      const s = splitScreen(i - 1, cell.terminal);
      const side = s.side();
      s.bind(null);
      s.bind(side);
    }
    cell.shown = visible;
  }
  const rects = slotRects(sizes());
  const split = wanted > 1;
  for (let i = 0; i < wanted; i++) {
    const cell = cells[i];
    const pane = paneAt(i);
    if (!cell) continue;
    cell.head?.remove();
    cell.head = split && pane ? head(i, pane) : null;
    if (cell.head) cell.box.prepend(cell.head);
    showBrowser(cell, pane?.browser ? pane.pane : null);
    const rect = rects.get(i);
    if (rect) place(cell.box, rect);
    const keyed = state.rail.row.keyed === i;
    cell.box.classList.toggle('keyed', split && keyed);
  }
  drawGutters(split);
}

function drawGutters(split: boolean): void {
  for (const old of splitEl().querySelectorAll('.gutter')) old.remove();
  if (!split) return;
  const { grid } = state.rail.row;
  const of = sizes();
  const rects = cellRects(of.heights, of.widths);
  grid.forEach((row, r) => {
    const line = rects[r];
    if (!line) return;
    line.slice(0, -1).forEach((rect, c) => {
      const g = el('div', {
        class: 'gutter vertical',
        role: 'separator',
        'aria-orientation': 'vertical',
        'aria-label': 'Resize the panes',
        title: 'Drag to resize',
      });
      g.style.left = percent(rect.x + rect.w);
      g.style.top = percent(rect.y);
      g.style.height = percent(rect.h);
      grip(g, 'x', r, c);
      splitEl().append(g);
    });
    const next = rects[r + 1]?.[0];
    if (next) {
      const g = el('div', {
        class: 'gutter horizontal',
        role: 'separator',
        'aria-orientation': 'horizontal',
        'aria-label': 'Resize the panes',
        title: 'Drag to resize',
      });
      g.style.top = percent(next.y);
      grip(g, 'y', r, 0);
      splitEl().append(g);
    }
  });
}

/** The fewest pixels a pane may have across (`x`) or down (`y`): the
 *  columns and rows Rust says, in the cells it is drawn in, and the room
 *  the terminal leaves round its grid. */
function minimum(axis: 'x' | 'y'): number {
  const { minCols, minRows } = state.rail.row;
  const cell = screen.cellSize();
  const style = getComputedStyle($('terminal'));
  if (axis === 'x') {
    return minCols * cell.w + parseFloat(style.paddingLeft) + parseFloat(style.paddingRight);
  }
  return (
    minRows * cell.h + parseFloat(style.paddingTop) + parseFloat(style.paddingBottom) + HEAD_PX
  );
}

/** Makes `g` drag the gutter after pane `at` of row `r` (`x`), or the one
 *  under row `r` (`y`). */
function grip(g: HTMLElement, axis: 'x' | 'y', r: number, at: number): void {
  g.addEventListener('dblclick', () => call('even_shares', { row: axis === 'x' ? r : null }));
  g.addEventListener('mousedown', (e) => {
    if (e.button !== 0) return;
    e.preventDefault();
    const box = splitEl().getBoundingClientRect();
    const total = axis === 'x' ? box.width : box.height;
    const now = sizes();
    const start = (axis === 'x' ? (now.widths[r] ?? []) : now.heights).map(
      (t) => (t / 1000) * total,
    );
    const from = axis === 'x' ? e.clientX : e.clientY;
    const least = minimum(axis);
    let latest: Sizes = now;
    let frame = 0;
    busy = true;
    document.body.classList.add('resizing');
    g.classList.add('held');
    const draw = (): void => {
      frame = 0;
      const rects = slotRects(latest);
      cells.forEach((cell, i) => {
        const rect = rects.get(i);
        if (rect && !cell.box.hidden) place(cell.box, rect);
      });
      g.style.cssText = '';
      const lines = cellRects(latest.heights, latest.widths);
      const rect = lines[r]?.[at];
      const next = lines[r + 1]?.[0];
      if (axis === 'x' && rect) {
        g.style.left = percent(rect.x + rect.w);
        g.style.top = percent(rect.y);
        g.style.height = percent(rect.h);
      } else if (next) {
        g.style.top = percent(next.y);
      }
    };
    const move = (m: MouseEvent): void => {
      const dragged = toThousandths(
        dragGutter(start, at, (axis === 'x' ? m.clientX : m.clientY) - from, least),
      );
      latest =
        axis === 'x'
          ? { heights: now.heights, widths: now.widths.map((w, i) => (i === r ? dragged : w)) }
          : { heights: dragged, widths: now.widths };
      if (!frame) frame = requestAnimationFrame(draw);
    };
    const up = (): void => {
      window.removeEventListener('mousemove', move);
      window.removeEventListener('mouseup', up);
      cancelAnimationFrame(frame);
      busy = false;
      held = latest;
      clearTimeout(heldTimer);
      // If Rust declines the sizes it says nothing, so the page lets go of
      // them rather than keep a layout Rust does not have.
      heldTimer = setTimeout(() => {
        held = null;
        drawSplit();
      }, 600);
      document.body.classList.remove('resizing');
      drawSplit();
      call('set_shares', { heights: latest.heights, widths: latest.widths });
    };
    window.addEventListener('mousemove', move);
    window.addEventListener('mouseup', up);
  });
}

// ---- Moving a pane by its header -------------------------------------------

/** Where a header held over the cells would land: the cell under the
 *  pointer, and the zone of it. */
function target(
  x: number,
  y: number,
  self: number,
): { slot: number; zone: Zone; box: DOMRect } | null {
  for (let i = 0; i < cells.length; i++) {
    const cell = cells[i];
    if (!cell || cell.box.hidden || i === self) continue;
    const box = cell.box.getBoundingClientRect();
    if (x < box.left || x >= box.right || y < box.top || y >= box.bottom) continue;
    return { slot: i, zone: dropZone((x - box.left) / box.width, (y - box.top) / box.height), box };
  }
  return null;
}

/** Where a dragged pane was let go: on a cell's zone, or over the stage
 *  on no cell, or neither. */
type Landing = { onto: Shown; zone: Zone } | { checkout: number } | 'stage' | null;

/** Drags a pane by a press at `e`, lighting the zone of the cell under the
 *  pointer, and hands `drop` where it was let go. `slot` is the cell the
 *  pane is already in, which is never a target, or -1 for one coming from
 *  the rail. `from` is the checkout a rail row's pane is in, which makes
 *  the other checkouts' rows targets; null for a drag that has none. Esc
 *  cancels. */
function dragPane(
  e: MouseEvent,
  slot: number,
  from: number | null,
  drop: (landing: Landing) => void,
): void {
  if (e.button !== 0) return;
  const origin = [e.clientX, e.clientY];
  let marker: HTMLElement | null = null;
  let over: ReturnType<typeof target> = null;
  let onStage = false;
  let row: HTMLElement | null = null;
  const lit = (m: MouseEvent): void => {
    if (from !== null) {
      const hit = document
        .elementFromPoint(m.clientX, m.clientY)
        ?.closest<HTMLElement>('[data-checkout]');
      const next = hit && Number(hit.dataset.checkout) !== from ? hit : null;
      if (next !== row) {
        row?.classList.remove('drop-target');
        next?.classList.add('drop-target');
        row = next;
      }
    }
    over = splitEl().hidden ? null : target(m.clientX, m.clientY, slot);
    const stage = $('body').getBoundingClientRect();
    onStage =
      m.clientX >= stage.left &&
      m.clientX < stage.right &&
      m.clientY >= stage.top &&
      m.clientY < stage.bottom;
    document.body.classList.toggle('dropping-on-stage', onStage && !over);
    if (!over) {
      marker?.remove();
      marker = null;
      return;
    }
    const area = splitEl().getBoundingClientRect();
    const zone = zoneRect(over.zone);
    marker ??= splitEl().appendChild(el('div', { class: 'drop-zone' }));
    marker.style.left = `${over.box.left - area.left + zone.x * over.box.width}px`;
    marker.style.top = `${over.box.top - area.top + zone.y * over.box.height}px`;
    marker.style.width = `${zone.w * over.box.width}px`;
    marker.style.height = `${zone.h * over.box.height}px`;
  };
  const move = (m: MouseEvent): void => {
    if (
      !busy &&
      Math.hypot(m.clientX - (origin[0] ?? 0), m.clientY - (origin[1] ?? 0)) < MOVE_THRESHOLD
    ) {
      return;
    }
    busy = true;
    document.body.classList.add('moving-pane');
    lit(m);
  };
  const end = (dropped: boolean): void => {
    window.removeEventListener('mousemove', move);
    window.removeEventListener('mouseup', up);
    window.removeEventListener('keydown', key, true);
    document.body.classList.remove('moving-pane', 'dropping-on-stage');
    marker?.remove();
    row?.classList.remove('drop-target');
    const moved = busy;
    const onto = over && cells[over.slot] ? paneAt(over.slot) : null;
    const landing: Landing = row
      ? { checkout: Number(row.dataset.checkout) }
      : onto && over
        ? { onto, zone: over.zone }
        : onStage
          ? 'stage'
          : null;
    busy = false;
    if (dropped && moved) drop(landing);
    else drawSplit();
  };
  const up = (): void => end(true);
  const key = (k: KeyboardEvent): void => {
    if (k.key !== 'Escape') return;
    k.preventDefault();
    k.stopPropagation();
    end(false);
  };
  window.addEventListener('mousemove', move);
  window.addEventListener('mouseup', up);
  window.addEventListener('keydown', key, true);
}

/** A header dragged onto another pane moves it there. */
function startMove(e: MouseEvent, slot: number, pane: number): void {
  dragPane(e, slot, null, (landing) => {
    if (landing && landing !== 'stage' && 'onto' in landing)
      call('move_pane', { pane, onto: landing.onto.pane, zone: landing.zone });
    else drawSplit();
  });
}

/** A pane dragged from the rail and let go over the stage opens there: on
 *  a pane's zone it lands as a header's drop would put it, and anywhere
 *  else on the stage it joins the grid by the grid rule. A click is left to
 *  the row, since no drag began. Let go on another checkout's row it moves
 *  there instead, when `checkout` says where it is now: the daemon answers
 *  with a tree, and the rail changes then. Null for a pane that cannot move
 *  between checkouts, a browser's. */
export function dragFromRail(e: MouseEvent, pane: number, checkout: number | null = null): void {
  // No text is selected across the rail while the pane is carried.
  if (e.button === 0) e.preventDefault();
  dragPane(e, -1, checkout, (landing) => {
    if (landing === 'stage') call('drop_pane', { pane, onto: null, zone: 'centre' });
    else if (landing && 'checkout' in landing)
      call('move_to_checkout', { pane, checkout: landing.checkout });
    else if (landing && landing.onto.pane !== pane) {
      call('drop_pane', { pane, onto: landing.onto.pane, zone: landing.zone });
    } else drawSplit();
  });
}
