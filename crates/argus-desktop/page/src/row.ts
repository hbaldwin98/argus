// A row view block: values and buttons on one line, wrapping when narrow.

import { el } from './dom.js';
import type { Block } from './bindings/Block';
import type { ViewCtx } from './viewctx.js';

/** Not drawn yet: the block's kind, so a view that uses it still reads. */
export function row(_block: Extract<Block, { kind: 'row' }>, _ctx: ViewCtx): HTMLElement {
  return el('div', { class: 'vpending' }, 'row');
}
