// A tree view block: nested nodes that fold.
import { chip } from './chip.js';
import { el, icon } from './dom.js';
import { value } from './values.js';
// Which nodes the person opened or folded, keyed by the node's place under
// its view. A view is redrawn whole, so this is the page's alone to keep;
// the node's own `open` only says how it starts.
const opened = new Map();
function fold(group, open) {
    const button = group.querySelector(':scope > .vtree-row > button');
    const children = group.querySelector(':scope > .vtree-children');
    if (!button || !children)
        return;
    button.setAttribute('aria-expanded', String(open));
    button.replaceChildren(icon(open ? 'down' : 'right'));
    children.hidden = !open;
}
function rows(nodes, ctx, prefix, depth) {
    return nodes.map((node, i) => {
        const key = `${prefix}/${i}`;
        const branch = node.children.length > 0;
        const open = branch && (opened.get(key) ?? node.open);
        const head = el('div', { class: 'vtree-row', style: `--depth:${depth}` }, branch
            ? el('button', {
                class: 'vtree-chev',
                type: 'button',
                'aria-expanded': String(open),
                'aria-label': node.label,
                onclick: (event) => {
                    const button = event.currentTarget;
                    const now = button.getAttribute('aria-expanded') !== 'true';
                    opened.set(key, now);
                    fold(button.closest('.vtree-node'), now);
                },
            }, icon(open ? 'down' : 'right'))
            : el('span', { class: 'vtree-chev', 'aria-hidden': 'true' }), el('span', { class: 'vtree-label' }, node.label), node.value && el('span', { class: 'vtree-value' }, value(node.value, ctx)), node.chip && el('span', { class: 'vtree-chip' }, chip(node.chip)));
        const children = branch
            ? el('div', { class: 'vtree-children' }, rows(node.children, ctx, key, depth + 1))
            : null;
        if (children)
            children.hidden = !open;
        return el('div', { class: 'vtree-node' }, head, children);
    });
}
export function tree(block, ctx) {
    return el('div', { class: 'vtree' }, rows(block.nodes, ctx, ctx.path, 0));
}
