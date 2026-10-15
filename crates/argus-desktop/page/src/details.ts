// A details view block: a summary line that opens to its blocks.

import { el } from './dom.js';
import type { Block } from './bindings/Block';
import type { ViewCtx } from './viewctx.js';

/** Not drawn yet: the block's kind, so a view that uses it still reads. */
export function details(_block: Extract<Block, { kind: 'details' }>, _ctx: ViewCtx): HTMLElement {
  return el('div', { class: 'vpending' }, 'details');
}
