// A diff view block: a unified diff drawn as the review viewer draws one.

import { el } from './dom.js';
import type { Block } from './bindings/Block';

/** Not drawn yet: the block's kind, so a view that uses it still reads. */
export function patch(_block: Extract<Block, { kind: 'diff' }>): HTMLElement {
  return el('div', { class: 'vpending' }, 'diff');
}
