// A tree view block: nested nodes that fold.

import { el } from './dom.js';
import type { Block } from './bindings/Block';
import type { ViewCtx } from './viewctx.js';

/** Not drawn yet: the block's kind, so a view that uses it still reads. */
export function tree(_block: Extract<Block, { kind: 'tree' }>, _ctx: ViewCtx): HTMLElement {
  return el('div', { class: 'vpending' }, 'tree');
}
