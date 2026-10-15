// A checklist view block: what is done and what is not.
import { el } from './dom.js';
/** Not drawn yet: the block's kind, so a view that uses it still reads. */
export function checklist(_block) {
    return el('div', { class: 'vpending' }, 'checklist');
}
