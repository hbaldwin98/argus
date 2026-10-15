// STUB: the values agent replaces this file.

import type { Chip } from './bindings/Chip';
import type { Tone } from './bindings/Tone';
import { el } from './dom.js';

/** The class that colours something by its tone: `t-good`. */
export function toneClass(tone: Tone): string {
  return `t-${tone}`;
}

export function chip(c: Chip): HTMLElement {
  return el('span', { class: `vchip ${toneClass(c.tone)}` }, c.label);
}
