// The pictures a browser's pages come in, and the canvases that show them.
// They come apart, many a second, and are painted onto each canvas showing
// that browser as they come. The page's CSP allows no `data:` image, so a
// picture is decoded to a bitmap and drawn rather than set as a source.

import type { BrowserShot } from '../bindings/BrowserShot';
import { el } from '../kit/dom.js';
import { within } from './input.js';

/** Where a browser is drawn: a tile on the Browsers stage, or its own cell
 *  in the grid. Each has a canvas of its own. */
export type Place = 'tile' | 'cell';
const PLACES: readonly Place[] = ['tile', 'cell'];

const shots = new Map<number, BrowserShot>();
const canvases = new Map<string, HTMLCanvasElement>();
/** The newest picture each browser has started decoding, so one that
 *  finishes after a newer one is dropped. */
const painting = new Map<number, number>();

/** A browser's canvas in one place, made once; `setup` runs on the new
 *  one, to give it the hand a large place answers with. */
export function canvasFor(
  pane: number,
  place: Place,
  setup?: (canvas: HTMLCanvasElement) => void,
): HTMLCanvasElement {
  const held = canvases.get(`${pane}:${place}`);
  if (held) return held;
  const canvas = el('canvas', { class: 'bscreen' }) as HTMLCanvasElement;
  setup?.(canvas);
  canvases.set(`${pane}:${place}`, canvas);
  return canvas;
}

export function hearShot(shot: BrowserShot): void {
  shots.set(shot.pane, shot);
  paint(shot.pane);
}

/** Lets go of a browser that closed. */
export function forgetShots(pane: number): void {
  shots.delete(pane);
  painting.delete(pane);
  for (const place of PLACES) canvases.delete(`${pane}:${place}`);
}

export function shotPanes(): number[] {
  return [...shots.keys()];
}

/** The point under the mouse in the page's CSS pixels, which the browser
 *  takes its input in. */
export function point(
  pane: number,
  canvas: HTMLCanvasElement,
  e: MouseEvent,
): { x: number; y: number } | null {
  const shot = shots.get(pane);
  const box = canvas.getBoundingClientRect();
  if (!shot || box.width === 0 || box.height === 0) return null;
  return within(box, shot, e.clientX, e.clientY);
}

/** Paints `pane`'s latest picture on every canvas on screen showing it. */
export function paint(pane: number): void {
  const shot = shots.get(pane);
  const showing = PLACES.map((place) => canvases.get(`${pane}:${place}`)).filter(
    (canvas): canvas is HTMLCanvasElement => !!canvas?.isConnected,
  );
  if (!shot || showing.length === 0) return;
  const ticket = (painting.get(pane) ?? 0) + 1;
  painting.set(pane, ticket);
  const bytes = Uint8Array.from(atob(shot.jpeg), (c) => c.charCodeAt(0));
  createImageBitmap(new Blob([bytes], { type: 'image/jpeg' }))
    .then((bitmap) => {
      if (painting.get(pane) !== ticket) return bitmap.close();
      for (const canvas of showing) {
        if (canvas.width !== bitmap.width) canvas.width = bitmap.width;
        if (canvas.height !== bitmap.height) canvas.height = bitmap.height;
        canvas.getContext('2d')?.drawImage(bitmap, 0, 0);
      }
      bitmap.close();
    })
    .catch(() => {});
}
