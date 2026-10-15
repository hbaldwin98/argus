// STUB: the list agent replaces this file.

import type { Item } from './bindings/Item';
import { el } from './dom.js';

export function list(items: readonly Item[]): HTMLElement {
  return el('div', { class: 'vlist' }, String(items.length));
}
