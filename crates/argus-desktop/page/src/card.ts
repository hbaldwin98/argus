// A card view block: blocks in a bordered box with a heading, a chip and
// buttons of its own.

import { el } from './dom.js';
import type { Block } from './bindings/Block';
import type { ViewCtx } from './viewctx.js';

/** Not drawn yet: the block's kind, so a view that uses it still reads. */
export function card(_block: Extract<Block, { kind: 'card' }>, _ctx: ViewCtx): HTMLElement {
  return el('div', { class: 'vpending' }, 'card');
}
