// A timeline view block: events in order, in lanes when they name one.
import { el } from './dom.js';
/** Not drawn yet: the block's kind, so a view that uses it still reads. */
export function timeline(_block) {
    return el('div', { class: 'vpending' }, 'timeline');
}
