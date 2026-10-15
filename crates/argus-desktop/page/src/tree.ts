// A tree view block: nested nodes that fold.

import { chip } from './chip.js';
import { el, icon } from './dom.js';
import { value } from './values.js';
import type { Block } from './bindings/Block';
import type { TreeNode } from './bindings/TreeNode';
import type { ViewCtx } from './viewctx.js';

// Which nodes the person opened or folded, keyed by the node's place under
// its view. A view is redrawn whole, so this is the page's alone to keep;
// the node's own `open` only says how it starts.
const opened = new Map<string, boolean>();

function fold(group: HTMLElement, open: boolean): void {
  const button = group.querySelector<HTMLElement>(':scope > .vtree-row > button');
  const children = group.querySelector<HTMLElement>(':scope > .vtree-children');
  if (!button || !children) return;
  button.setAttribute('aria-expanded', String(open));
  button.replaceChildren(icon(open ? 'down' : 'right'));
  children.hidden = !open;
}

function rows(
  nodes: readonly TreeNode[],
  ctx: ViewCtx,
  prefix: string,
  depth: number,
): HTMLElement[] {
  return nodes.map((node, i) => {
    const key = `${prefix}/${i}`;
    const branch = node.children.length > 0;
    const open = branch && (opened.get(key) ?? node.open);
    const head = el(
      'div',
      { class: 'vtree-row', style: `--depth:${depth}` },
      branch
        ? el(
            'button',
            {
              class: 'vtree-chev',
              type: 'button',
              'aria-expanded': String(open),
              'aria-label': node.label,
              onclick: (event: Event) => {
                const button = event.currentTarget as HTMLElement;
                const now = button.getAttribute('aria-expanded') !== 'true';
                opened.set(key, now);
                fold(button.closest<HTMLElement>('.vtree-node')!, now);
              },
            },
            icon(open ? 'down' : 'right'),
          )
        : el('span', { class: 'vtree-chev', 'aria-hidden': 'true' }),
      el('span', { class: 'vtree-label' }, node.label),
      node.value && el('span', { class: 'vtree-value' }, value(node.value, ctx)),
      node.chip && el('span', { class: 'vtree-chip' }, chip(node.chip)),
    );
    const children = branch
      ? el('div', { class: 'vtree-children' }, rows(node.children, ctx, key, depth + 1))
      : null;
    if (children) children.hidden = !open;
    return el('div', { class: 'vtree-node' }, head, children);
  });
}

export function tree(block: Extract<Block, { kind: 'tree' }>, ctx: ViewCtx): HTMLElement {
  return el('div', { class: 'vtree' }, rows(block.nodes, ctx, ctx.path, 0));
}
