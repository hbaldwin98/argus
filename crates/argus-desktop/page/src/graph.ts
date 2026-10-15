// A graph view block: boxes and arrows the page lays out.

import { el } from './dom.js';
import type { Block } from './bindings/Block';

/** Not drawn yet: the block's kind, so a view that uses it still reads. */
export function graph(_block: Extract<Block, { kind: 'graph' }>): HTMLElement {
  return el('div', { class: 'vpending' }, 'graph');
}
