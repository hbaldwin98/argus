// STUB: the callout agent replaces this file.

import { el } from './dom.js';

export function code(lang: string | null, text: string): HTMLElement {
  return el('pre', { class: 'vcode' }, lang ?? '', text);
}
