// STUB: the list agent replaces this file.

import type { Stat } from './bindings/Stat';
import { el } from './dom.js';

export function stats(list: readonly Stat[]): HTMLElement {
  return el('div', { class: 'vstats' }, String(list.length));
}
