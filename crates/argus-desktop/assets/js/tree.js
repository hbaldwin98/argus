// A tree view block: nested nodes that fold.
import { el } from './dom.js';
/** Not drawn yet: the block's kind, so a view that uses it still reads. */
export function tree(_block, _ctx) {
    return el('div', { class: 'vpending' }, 'tree');
}
