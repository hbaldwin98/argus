// A grid view block: as many blocks side by side as fit at a named
// minimum width, wrapping onto rows.

import type { Block } from './bindings/Block';
import { blocks } from './blocks.js';
import { el } from './dom.js';
import { at, type ViewCtx } from './viewctx.js';

/** The agent names a width and the page owns the pixels (`.vgrid.sm`). */
export function grid(block: Extract<Block, { kind: 'grid' }>, ctx: ViewCtx): HTMLElement {
  return el(
    'div',
    { class: `vgrid ${block.min}` },
    block.items.map((item, i) => blocks([item], at(ctx, i))),
  );
}
