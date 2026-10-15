// STUB: the values agent replaces this file.

import type { Field } from './bindings/Field';
import { el } from './dom.js';

export function fields(list: readonly Field[]): HTMLElement {
  return el('dl', { class: 'vfields' }, list.map((f) => f.label));
}
