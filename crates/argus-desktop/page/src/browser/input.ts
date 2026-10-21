// What the person's hand on a browser's picture becomes: a point in the
// page's own pixels, a key or text, an address. Nothing here reads the
// DOM, so it is tested under node.

import type { BrowserInput } from '../bindings/BrowserInput';
import type { BrowserKey } from '../bindings/BrowserKey';

/** What the person typed, as an address the daemon will open: a bare host
 *  is taken as the web. */
export function address(typed: string): string {
  const text = typed.trim();
  if (/^[a-z][a-z0-9+.-]*:/i.test(text)) return text;
  return `https://${text}`;
}

/** A point in a box the page's picture is fitted into whole, keeping its
 *  shape (`object-fit: contain`), as the page's own CSS pixels; `null` in
 *  the bands either side of it. */
export function within(
  box: { left: number; top: number; width: number; height: number },
  page: { width: number; height: number },
  x: number,
  y: number,
): { x: number; y: number } | null {
  const scale = Math.min(box.width / page.width, box.height / page.height);
  const left = box.left + (box.width - page.width * scale) / 2;
  const top = box.top + (box.height - page.height * scale) / 2;
  const at = { x: (x - left) / scale, y: (y - top) / scale };
  if (at.x < 0 || at.y < 0 || at.x > page.width || at.y > page.height) return null;
  return at;
}

const KEYS: Partial<Record<string, BrowserKey>> = {
  Enter: 'Enter',
  Backspace: 'Backspace',
  Delete: 'Delete',
  Tab: 'Tab',
  Escape: 'Escape',
  ArrowUp: 'ArrowUp',
  ArrowDown: 'ArrowDown',
  ArrowLeft: 'ArrowLeft',
  ArrowRight: 'ArrowRight',
  Home: 'Home',
  End: 'End',
  PageUp: 'PageUp',
  PageDown: 'PageDown',
};

/** A key as the browser takes it: a named key, or text typed. A chord with
 *  Ctrl or Meta is the window's. */
export function keyInput(e: {
  key: string;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
}): BrowserInput | null {
  if (e.ctrlKey || e.metaKey) return null;
  const key = KEYS[e.key];
  if (key) return { kind: 'key', key };
  if ([...e.key].length === 1) return { kind: 'text', text: e.key };
  return null;
}
