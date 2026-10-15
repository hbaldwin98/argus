// A row view block: values and buttons on one line, wrapping when narrow.
import { actionButton } from './actions.js';
import { el } from './dom.js';
import { value } from './values.js';
import { at } from './viewctx.js';
export function row(block, ctx) {
    return el('div', { class: `vrow ${block.align}` }, block.items.map((item, i) => item.kind === 'value' ? value(item.value, at(ctx, i)) : actionButton(item.action, at(ctx, i))));
}
