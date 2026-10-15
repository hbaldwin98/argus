// A view's blocks, each handed to the module that draws its kind; only
// the layout blocks — sections, columns, tabs — are drawn here, since they
// hold blocks themselves. Which tab is chosen is the page's alone and
// outlives a redraw: a view that comes again keeps the tab being read.

import type { Block } from './bindings/Block';
import { board } from './board.js';
import { callout } from './callout.js';
import { card } from './card.js';
import { chart } from './chart.js';
import { checklist } from './checklist.js';
import { code } from './code.js';
import { details } from './details.js';
import { diagram } from './diagram.js';
import { el } from './dom.js';
import { fields } from './fields.js';
import { form } from './form.js';
import { graph } from './graph.js';
import { grid } from './grid.js';
import { list } from './list.js';
import { log } from './log.js';
import { markdown } from './markdown.js';
import { matrix } from './matrix.js';
import { patch } from './patch.js';
import { progress } from './progress.js';
import { row } from './row.js';
import { stats } from './stats.js';
import { table } from './table.js';
import { timeline } from './timeline.js';
import { tree } from './tree.js';
import { at, type ViewCtx } from './viewctx.js';

/** The tab chosen in each tabs block, by the view and the block's path. */
const chosenTabs = new Map<string, number>();

export function blocks(all: readonly Block[], ctx: ViewCtx): HTMLElement {
  return el(
    'div',
    { class: 'vblocks' },
    all.map((block, i) => one(block, at(ctx, i))),
  );
}

function one(block: Block, ctx: ViewCtx): HTMLElement {
  switch (block.kind) {
    case 'section':
      return el(
        'section',
        { class: 'vsection' },
        el('div', { class: 'label' }, block.heading),
        blocks(block.blocks, ctx),
      );
    case 'columns':
      return el(
        'div',
        { class: 'vcolumns' },
        block.columns.map((column, i) => blocks(column, at(ctx, i))),
      );
    case 'tabs':
      return tabs(block, ctx);
    case 'markdown':
      return markdown(block.text);
    case 'fields':
      return fields(block.fields, ctx);
    case 'table':
      return table(block, null, ctx);
    case 'list':
      return list(block.items, ctx);
    case 'stats':
      return stats(block.stats);
    case 'code':
      return code(block.lang, block.text);
    // Read in Rust with the rest of the view; a source it could not read
    // is still what the agent meant, and readable.
    case 'diagram': {
      const read = ctx.drawn(block.source);
      return read ? diagram(read) : code('mermaid', block.source);
    }
    case 'callout':
      return callout(block.tone, block.text);
    case 'card':
      return card(block, ctx);
    case 'grid':
      return grid(block, ctx);
    case 'details':
      return details(block, ctx);
    case 'row':
      return row(block, ctx);
    case 'divider':
      return el('hr', { class: 'vdivider' });
    case 'chart':
      return chart(block);
    case 'progress':
      return progress(block);
    case 'timeline':
      return timeline(block);
    case 'tree':
      return tree(block, ctx);
    case 'graph':
      return graph(block);
    case 'diff':
      return patch(block);
    case 'matrix':
      return matrix(block);
    case 'checklist':
      return checklist(block);
    case 'log':
      return log(block, ctx);
    case 'board':
      return board(block, ctx);
    case 'form':
      return form(block, ctx);
  }
}

function tabs(block: Extract<Block, { kind: 'tabs' }>, ctx: ViewCtx): HTMLElement {
  const chosen = Math.min(chosenTabs.get(ctx.path) ?? 0, block.tabs.length - 1);
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
    chosenTabs.set(ctx.path, i);
    buttons.forEach((b, j) => {
      b.classList.toggle('on', i === j);
      b.setAttribute('aria-selected', String(i === j));
    });
    const tab = block.tabs[i];
    body.replaceChildren(tab ? blocks(tab.blocks, at(ctx, i)) : '');
  };
  pick(chosen);
  return el(
    'div',
    { class: 'vtabs' },
    el('div', { class: 'lens vtabs-bar', role: 'tablist' }, buttons),
    body,
  );
}
