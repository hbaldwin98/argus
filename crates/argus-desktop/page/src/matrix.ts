// A matrix view block: a grid of tinted cells.

import { el } from './dom.js';
import type { Block } from './bindings/Block';

/** Not drawn yet: the block's kind, so a view that uses it still reads. */
export function matrix(_block: Extract<Block, { kind: 'matrix' }>): HTMLElement {
  return el('div', { class: 'vpending' }, 'matrix');
}
