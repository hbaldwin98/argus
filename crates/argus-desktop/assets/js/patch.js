// A diff view block: a unified diff drawn as the review viewer draws one.
import { el } from './dom.js';
/** Not drawn yet: the block's kind, so a view that uses it still reads. */
export function patch(_block) {
    return el('div', { class: 'vpending' }, 'diff');
}
