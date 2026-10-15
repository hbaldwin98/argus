// A checklist view block: what is done and what is not.

import { el } from './dom.js';
import type { Block } from './bindings/Block';

/** Not drawn yet: the block's kind, so a view that uses it still reads. */
export function checklist(_block: Extract<Block, { kind: 'checklist' }>): HTMLElement {
  return el('div', { class: 'vpending' }, 'checklist');
}
