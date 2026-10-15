// A grid view block: as many blocks side by side as fit at a named
// minimum width, wrapping onto rows.
import { el } from './dom.js';
/** Not drawn yet: the block's kind, so a view that uses it still reads. */
export function grid(_block, _ctx) {
    return el('div', { class: 'vpending' }, 'grid');
}
