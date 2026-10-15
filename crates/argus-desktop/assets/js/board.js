// A board view block: columns of cards.
import { el } from './dom.js';
/** Not drawn yet: the block's kind, so a view that uses it still reads. */
export function board(_block) {
    return el('div', { class: 'vpending' }, 'board');
}
