// A card view block: blocks in a bordered box with a heading, a chip and
// buttons of its own.
import { el } from './dom.js';
/** Not drawn yet: the block's kind, so a view that uses it still reads. */
export function card(_block, _ctx) {
    return el('div', { class: 'vpending' }, 'card');
}
