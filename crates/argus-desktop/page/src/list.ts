// A list view block: comments, events, commits. An item with no `who` is an
// event, drawn as one muted line under a dot so it never reads as a comment.

import type { Item } from './bindings/Item';
import { chip } from './chip.js';
import { el } from './dom.js';
import { markdown } from './markdown.js';
import { ago, person } from './values.js';
import type { ViewCtx } from './viewctx.js';

export function list(items: readonly Item[], ctx: ViewCtx | null = null): HTMLElement {
  void ctx;
  if (items.length === 0) return el('div', { class: 'vlist vlist-empty' }, 'Nothing yet');
  return el('div', { class: 'vlist' }, items.map(row));
}

function row(item: Item): HTMLElement {
  const event = item.who == null;
  const meta = [
    item.who != null ? person(item.who) : null,
    item.at != null ? el('span', { class: 'vlist-at', title: item.at }, ago(item.at)) : null,
    item.chip ? chip(item.chip) : null,
  ];
  return el(
    'div',
    { class: event ? 'vlist-item vlist-event' : 'vlist-item' },
    event ? el('span', { class: 'vlist-dot', 'aria-hidden': 'true' }, '●') : null,
    el(
      'div',
      { class: 'vlist-main' },
      el('div', { class: 'vlist-meta' }, item.title != null && event ? titleOf(item) : null, meta),
      item.title != null && !event ? titleOf(item) : null,
      item.body ? el('div', { class: 'vlist-body' }, markdown(item.body)) : null,
    ),
  );
}

function titleOf(item: Item): HTMLElement {
  return el('span', { class: 'vlist-title' }, item.title);
}
