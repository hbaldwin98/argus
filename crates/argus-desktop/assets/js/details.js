// A details view block: a summary line that opens to its blocks.
import { el } from './dom.js';
/** Not drawn yet: the block's kind, so a view that uses it still reads. */
export function details(_block, _ctx) {
    return el('div', { class: 'vpending' }, 'details');
}
