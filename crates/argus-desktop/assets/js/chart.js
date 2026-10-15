// A chart view block: series over a shared x axis, the axes and scale the
// page's own.
import { el } from './dom.js';
/** Not drawn yet: the block's kind, so a view that uses it still reads. */
export function chart(_block) {
    return el('div', { class: 'vpending' }, 'chart');
}
