// A log view block: a tail of lines, each with its level.

import { el } from './dom.js';
import type { Block } from './bindings/Block';

/** Not drawn yet: the block's kind, so a view that uses it still reads. */
export function log(_block: Extract<Block, { kind: 'log' }>): HTMLElement {
  return el('div', { class: 'vpending' }, 'log');
}
