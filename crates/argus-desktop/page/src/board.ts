// A board view block: columns of cards.

import { el } from './dom.js';
import type { Block } from './bindings/Block';

/** Not drawn yet: the block's kind, so a view that uses it still reads. */
export function board(_block: Extract<Block, { kind: 'board' }>): HTMLElement {
  return el('div', { class: 'vpending' }, 'board');
}
