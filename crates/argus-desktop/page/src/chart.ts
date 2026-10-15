// A chart view block: series over a shared x axis, the axes and scale the
// page's own.

import { el } from './dom.js';
import type { Block } from './bindings/Block';

/** Not drawn yet: the block's kind, so a view that uses it still reads. */
export function chart(_block: Extract<Block, { kind: 'chart' }>): HTMLElement {
  return el('div', { class: 'vpending' }, 'chart');
}
