// STUB: the table agent replaces this file.

import type { Block } from './bindings/Block';
import { el } from './dom.js';

export type TableBlock = Extract<Block, { kind: 'table' }>;

export function table(block: TableBlock): HTMLElement {
  return el('div', { class: 'vtable' }, String(block.rows.length));
}
