// A view's blocks, each handed to the module that draws its kind; only
// the layout blocks — sections, columns, tabs — are drawn here, since they
// hold blocks themselves. Which tab is chosen is the page's alone and
// outlives a redraw: a view that comes again keeps the tab being read.

import type { Block } from '../bindings/Block';
import { board } from './blocks/board.js';
import { callout } from './blocks/callout.js';
import { card } from './blocks/card.js';
import { chart } from './blocks/chart.js';
import { checklist } from './blocks/checklist.js';
import { code } from './blocks/code.js';
import { details } from './blocks/details.js';
import { diagram } from './blocks/diagram.js';
import { el } from '../kit/dom.js';
import { embed } from './blocks/embed.js';
import { fields } from './blocks/fields.js';
import { form } from './blocks/form.js';
import { graph } from './blocks/graph.js';
import { grid } from './blocks/grid.js';
import { list } from './blocks/list.js';
import { log } from './blocks/log.js';
import { markdown } from './blocks/markdown.js';
import { matrix } from './blocks/matrix.js';
import { patch } from './blocks/patch.js';
import { progress } from './blocks/progress.js';
import { row } from './blocks/row.js';
import { opensMark } from './saved/link.js';
import { stats } from './blocks/stats.js';
import { table } from './blocks/table.js';
import { timeline } from './blocks/timeline.js';
import { tree } from './blocks/tree.js';
import { at, type ViewCtx } from './ctx.js';

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
  const drawn = draw(block, ctx);
  // The path under the view is what follows `pane/id.` in the context's;
  // an id may hold dots itself, so the prefix is measured, not searched.
  if (ctx.changed?.size && ctx.changed.has(ctx.path.slice(`${ctx.pane}/${ctx.id}.`.length))) {
    drawn.classList.add('vchanged');
  }
  return drawn;
}

function draw(block: Block, ctx: ViewCtx): HTMLElement {
  switch (block.kind) {
    case 'section':
      return el(
        'section',
        { class: 'vsection' },
        el('div', { class: 'label' }, block.heading, opensMark(block.opens, ctx)),
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
      return stats(block.stats, ctx);
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
    case 'embed':
      return embed(block.view, ctx);
    case 'divider':
      return el('hr', { class: 'vdivider' });
    case 'chart':
      return chart(block);
    case 'progress':
      return progress(block);
    case 'timeline':
      return timeline(block, ctx);
    case 'tree':
      return tree(block, ctx);
    case 'graph':
      return graph(block, ctx);
    case 'diff':
      return patch(block);
    case 'matrix':
      return matrix(block);
    case 'checklist':
      return checklist(block, ctx);
    case 'log':
      return log(block, ctx);
    case 'board':
      return board(block, ctx);
    case 'form':
      return form(block, ctx);
    // A block a newer daemon sent that this window has no drawing for.
    case 'unknown':
      return el('div', { class: 'vmuted' }, 'This needs a newer Argus to show.');
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
