// A form view block: fields the person fills in, sent back to the agent
// with the form's submit action.
import { el } from './dom.js';
/** Not drawn yet: the block's kind, so a view that uses it still reads. */
export function form(_block, _ctx) {
    return el('div', { class: 'vpending' }, 'form');
}
