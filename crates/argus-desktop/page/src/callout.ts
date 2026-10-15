// STUB: the callout agent replaces this file.

import type { Tone } from './bindings/Tone';
import { el } from './dom.js';

export function callout(tone: Tone, text: string): HTMLElement {
  return el('div', { class: 'vcallout' }, tone, text);
}
