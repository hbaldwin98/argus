// A checklist view block: what is done and what is not.
import { el } from './dom.js';
import { opensMark } from './savedlink.js';
const CHECK = '<svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 8.5l3 3 5-6"></path></svg>';
/** Read-only: the agent keeps the list, the person only reads it. */
export function checklist(block, ctx) {
    const done = block.items.filter((item) => item.done).length;
    return el('div', { class: 'vcheck' }, el('div', { class: 'vcheck-count' }, `${done} of ${block.items.length} done`), el('ul', { class: 'vcheck-items' }, block.items.map((item) => {
        const box = el('span', { class: 'vcheck-box', 'aria-hidden': 'true' });
        // The glyph is a constant of this file, not an agent's string.
        if (item.done)
            box.innerHTML = CHECK;
        return el('li', { class: item.done ? 'vcheck-item done' : 'vcheck-item' }, box, el('span', { class: 'sr-only' }, item.done ? 'Done: ' : 'Not done: '), el('span', { class: 'vcheck-label' }, item.label), opensMark(item.opens, ctx), item.note && el('span', { class: 'vcheck-note' }, item.note));
    })));
}
