// A graph view block: boxes and arrows the page lays out.
import { el } from './dom.js';
/** Not drawn yet: the block's kind, so a view that uses it still reads. */
export function graph(_block) {
    return el('div', { class: 'vpending' }, 'graph');
}
