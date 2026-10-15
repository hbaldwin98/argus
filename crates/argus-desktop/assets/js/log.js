// A log view block: a tail of lines, each with its level.
import { el } from './dom.js';
/** Not drawn yet: the block's kind, so a view that uses it still reads. */
export function log(_block) {
    return el('div', { class: 'vpending' }, 'log');
}
