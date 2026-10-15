// A matrix view block: a grid of tinted cells.
import { el } from './dom.js';
/** Not drawn yet: the block's kind, so a view that uses it still reads. */
export function matrix(_block) {
    return el('div', { class: 'vpending' }, 'matrix');
}
