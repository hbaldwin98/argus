// A board view block: columns of cards.

import { chip } from './chip.js';
import { el } from './dom.js';
import { markdown } from './markdown.js';
import { value } from './values.js';
import type { Block } from './bindings/Block';
import type { BoardCard } from './bindings/BoardCard';
import type { ViewCtx } from './viewctx.js';

function card(c: BoardCard, ctx: ViewCtx): HTMLElement {
  return el(
    'div',
    { class: 'vboard-card' },
    el(
      'div',
      { class: 'vboard-card-head' },
      el('span', { class: 'vboard-card-title' }, c.title),
      c.chip && chip(c.chip),
    ),
    c.value && el('div', { class: 'vboard-card-value' }, value(c.value, ctx)),
    c.body && el('div', { class: 'vboard-card-body' }, markdown(c.body)),
  );
}

export function board(block: Extract<Block, { kind: 'board' }>, ctx: ViewCtx): HTMLElement {
  return el(
    'div',
    { class: 'vboard' },
    block.columns.map((column) =>
      el(
        'section',
        { class: 'vboard-col' },
        el(
          'header',
          { class: 'vboard-col-head' },
          el('span', { class: 'vboard-col-title' }, column.title),
          el('span', { class: 'vboard-count' }, column.cards.length),
          column.chip && chip(column.chip),
        ),
        el(
          'div',
          { class: 'vboard-cards' },
          column.cards.map((c) => card(c, ctx)),
        ),
      ),
    ),
  );
}
