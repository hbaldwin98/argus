// A view's blocks, each handed to the module that draws its kind; only
// the layout blocks — sections, columns, tabs — are drawn here, since they
// hold blocks themselves. Which tab is chosen is the page's alone and
// outlives a redraw: a view that comes again keeps the tab being read.

import type { Block } from './bindings/Block';
import { callout } from './callout.js';
import { code } from './code.js';
import { el } from './dom.js';
import { fields } from './fields.js';
import { list } from './list.js';
import { markdown } from './markdown.js';
import { stats } from './stats.js';
import { table } from './table.js';

/** The tab chosen in each tabs block, by the view and the block's path. */
const chosenTabs = new Map<string, number>();

export function blocks(all: readonly Block[], path: string): HTMLElement {
  return el(
    'div',
    { class: 'vblocks' },
    all.map((block, i) => one(block, `${path}.${i}`)),
  );
}

function one(block: Block, path: string): HTMLElement {
  switch (block.kind) {
    case 'section':
      return el(
        'section',
        { class: 'vsection' },
        el('div', { class: 'label' }, block.heading),
        blocks(block.blocks, path),
      );
    case 'columns':
      return el(
        'div',
        { class: 'vcolumns' },
        block.columns.map((column, i) => blocks(column, `${path}.${i}`)),
      );
    case 'tabs':
      return tabs(block, path);
    case 'markdown':
      return markdown(block.text);
    case 'fields':
      return fields(block.fields);
    case 'table':
      return table(block);
    case 'list':
      return list(block.items);
    case 'stats':
      return stats(block.stats);
    case 'code':
      return code(block.lang, block.text);
    // The desktop has no diagram renderer yet; the source is still what
    // the agent meant, and readable.
    case 'diagram':
      return code('mermaid', block.source);
    case 'callout':
      return callout(block.tone, block.text);
  }
}

function tabs(block: Extract<Block, { kind: 'tabs' }>, path: string): HTMLElement {
  const chosen = Math.min(chosenTabs.get(path) ?? 0, block.tabs.length - 1);
  const body = el('div', { class: 'vtabs-body', role: 'tabpanel' });
  const buttons = block.tabs.map((tab, i) =>
    el(
      'button',
      {
        class: 'lt',
        role: 'tab',
        type: 'button',
        'aria-selected': String(i === chosen),
        onclick: () => pick(i),
      },
      tab.title,
    ),
  );
  const pick = (i: number): void => {
    chosenTabs.set(path, i);
    buttons.forEach((b, j) => {
      b.classList.toggle('on', i === j);
      b.setAttribute('aria-selected', String(i === j));
    });
    const tab = block.tabs[i];
    body.replaceChildren(tab ? blocks(tab.blocks, `${path}.${i}`) : '');
  };
  pick(chosen);
  return el(
    'div',
    { class: 'vtabs' },
    el('div', { class: 'lens vtabs-bar', role: 'tablist' }, buttons),
    body,
  );
}
