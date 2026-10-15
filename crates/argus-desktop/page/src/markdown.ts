// STUB: the markdown agent replaces this file.

import { el } from './dom.js';

/** Markdown drawn as elements, every string as text. */
export function markdown(text: string): HTMLElement {
  return el('div', { class: 'md' }, text);
}
