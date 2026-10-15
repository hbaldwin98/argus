// A row view block: values and buttons on one line, wrapping when narrow.
import { el } from './dom.js';
/** Not drawn yet: the block's kind, so a view that uses it still reads. */
export function row(_block, _ctx) {
    return el('div', { class: 'vpending' }, 'row');
}
