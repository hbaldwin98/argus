// The dialogs: saved views floating over the stage, each in a frame that
// its header moves and its corner resizes, the one pressed coming in front.
// Rust keeps where each sits, how big, and in what order
// (`src/layout/docks.rs`); this draws them there, follows the pointer while
// one is held, and reports where it was let go.

import type { FloatBounds } from '../bindings/FloatBounds';
import type { FloatPage } from '../bindings/FloatPage';
import type { Panel } from '../bindings/Panel';
import { $, el } from '../kit/dom.js';
import { call } from '../connection/tauri.js';

/** Each dialog's frame, kept across redraws so its panel keeps its state. */
const frames = new Map<Panel, HTMLElement>();
/** The dialog being moved or resized, which a redraw leaves where the
 *  pointer has it. */
let held: Panel | null = null;
/** The dialog in front, so a press on it sends nothing. */
let front: Panel | null = null;

/** How much of a dialog stays in the work area however it is moved, so its
 *  header can always be reached again. */
const KEEP = { x: 120, y: 40 };

/** Puts each dialog's panel in its frame, where and as big as Rust says,
 *  back to front; frames of panels no longer floating go, their panels
 *  parked. */
export function drawFloats(
  floating: readonly FloatPage[],
  element: (panel: Panel) => HTMLElement | null,
): void {
  const layer = $('floats');
  const kept = new Set<Panel>();
  floating.forEach((f, i) => {
    const panel = element(f.panel);
    if (!panel) return;
    kept.add(f.panel);
    const frame = frames.get(f.panel) ?? frameFor(f.panel);
    if (panel.parentElement !== frame) frame.prepend(panel);
    if (frame.parentElement !== layer) layer.append(frame);
    frame.style.zIndex = String(i + 1);
    frame.setAttribute('aria-label', f.title);
    if (held !== f.panel) place(frame, f.bounds);
  });
  front = floating.at(-1)?.panel ?? null;
  for (const [panel, frame] of frames) {
    if (kept.has(panel)) continue;
    for (const child of Array.from(frame.children)) {
      if (!child.classList.contains('float-grip')) $('parked').append(child);
    }
    frame.remove();
    frames.delete(panel);
  }
}

function frameFor(panel: Panel): HTMLElement {
  const grip = el('div', { class: 'float-grip', 'aria-hidden': 'true' });
  const frame = el('div', { class: 'float', role: 'dialog' }, grip);
  frame.addEventListener('mousedown', (down) => {
    if (down.button !== 0) return;
    if (front !== panel) call('docks', { gesture: { on: 'raise', panel } });
    const target = down.target instanceof Element ? down.target : null;
    if (target === grip) {
      hold(panel, frame, down, (box, dx, dy) => ({
        ...box,
        width: Math.max(280, box.width + dx),
        height: Math.max(160, box.height + dy),
      }));
      return;
    }
    const head = target?.closest('.saved-head');
    if (!head || target?.closest('button, input, textarea, a')) return;
    hold(panel, frame, down, (box, dx, dy) => ({ ...box, x: box.x + dx, y: box.y + dy }));
  });
  frames.set(panel, frame);
  return frame;
}

/** Follows the pointer from `down`, the frame's box changed by `to` for
 *  how far it went, and reports the box it was let go at. */
function hold(
  panel: Panel,
  frame: HTMLElement,
  down: MouseEvent,
  to: (box: FloatBounds, dx: number, dy: number) => FloatBounds,
): void {
  down.preventDefault();
  const start = boxOf(frame);
  let now = start;
  held = panel;
  document.body.classList.add('float-moving');
  const move = (m: MouseEvent): void => {
    now = to(start, m.clientX - down.clientX, m.clientY - down.clientY);
    place(frame, now);
  };
  const up = (): void => {
    window.removeEventListener('mousemove', move, true);
    window.removeEventListener('mouseup', up, true);
    document.body.classList.remove('float-moving');
    held = null;
    if (now !== start) call('docks', { gesture: { on: 'bounds', panel, bounds: shown(frame) } });
  };
  window.addEventListener('mousemove', move, true);
  window.addEventListener('mouseup', up, true);
}

/** Draws the frame at `box`, kept where its header can be reached and no
 *  bigger than the work area. */
function place(frame: HTMLElement, box: FloatBounds): void {
  const area = $('floats').getBoundingClientRect();
  const width = Math.min(box.width, Math.max(area.width, 0));
  const height = Math.min(box.height, Math.max(area.height, 0));
  const x = Math.min(Math.max(box.x, KEEP.x - width), Math.max(area.width - KEEP.x, 0));
  const y = Math.min(Math.max(box.y, 0), Math.max(area.height - KEEP.y, 0));
  Object.assign(frame.style, {
    left: `${x}px`,
    top: `${y}px`,
    width: `${width}px`,
    height: `${height}px`,
  });
}

function boxOf(frame: HTMLElement): FloatBounds {
  return {
    x: parseFloat(frame.style.left) || 0,
    y: parseFloat(frame.style.top) || 0,
    width: parseFloat(frame.style.width) || frame.offsetWidth,
    height: parseFloat(frame.style.height) || frame.offsetHeight,
  };
}

/** Where the frame is drawn, in whole pixels, as Rust keeps it. */
function shown(frame: HTMLElement): FloatBounds {
  const box = boxOf(frame);
  return {
    x: Math.round(box.x),
    y: Math.round(box.y),
    width: Math.round(box.width),
    height: Math.round(box.height),
  };
}
