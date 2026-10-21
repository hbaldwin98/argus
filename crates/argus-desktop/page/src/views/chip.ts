// A status pill. An agent picks a tone, a meaning, and the page owns the hue,
// so a view looks the same in every theme whoever wrote it.

import type { Chip } from '../bindings/Chip';
import type { Tone } from '../bindings/Tone';
import { el } from '../kit/dom.js';

/** The class that colours something by its tone: `t-good`. */
export function toneClass(tone: Tone): string {
  return `t-${tone}`;
}

/** The quiet form of a status in a dense table: a dot in the tone's colour
 *  and the word in muted text, the word itself coloured only for a bad one. */
export function dotted(c: Chip): HTMLElement {
  return el(
    'span',
    { class: `vdotted ${toneClass(c.tone)}` },
    el('span', { class: 'vdotted-dot', 'aria-hidden': 'true' }),
    el('span', { class: 'vdotted-word' }, c.label),
  );
}

export function chip(c: Chip): HTMLElement {
  return el('span', { class: `vchip ${toneClass(c.tone)}` }, c.label);
}
