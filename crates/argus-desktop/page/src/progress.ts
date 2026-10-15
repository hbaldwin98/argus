// A progress view block, and the meter a progress value draws in a cell.

import { el } from './dom.js';
import type { Block } from './bindings/Block';
import type { Value } from './bindings/Value';

/** Not drawn yet: a progress value in a cell. */
export function progressValue(v: Extract<Value, { kind: 'progress' }>): HTMLElement {
  return el('span', { class: 'vpending' }, `${v.value} / ${v.max}`);
}

/** Not drawn yet: the block's kind, so a view that uses it still reads. */
export function progress(_block: Extract<Block, { kind: 'progress' }>): HTMLElement {
  return el('div', { class: 'vpending' }, 'progress');
}
