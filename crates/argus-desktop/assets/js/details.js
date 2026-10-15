// A details view block: a summary line that opens to its blocks.
import { blocks } from './blocks.js';
import { chip } from './chip.js';
import { el } from './dom.js';
/** Which details are open, by the block's path. The agent's `open` is only
 *  how one starts: a view is redrawn whole on every refresh, and the one
 *  being read must not close under the reader. */
const opened = new Map();
export function details(block, ctx) {
    const node = el('details', { class: 'vdetails' }, el('summary', { class: 'vdetails-summary' }, el('span', { class: 'vdetails-text' }, block.summary), block.chip ? chip(block.chip) : null), el('div', { class: 'vdetails-body' }, blocks(block.blocks, ctx)));
    node.open = opened.get(ctx.path) ?? block.open;
    node.addEventListener('toggle', () => opened.set(ctx.path, node.open));
    return node;
}
