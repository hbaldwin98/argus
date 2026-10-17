// A list view block: comments, events, commits. An item with no `who` is an
// event, drawn as one muted line under a dot so it never reads as a comment.
import { chip } from './chip.js';
import { el } from './kit/dom.js';
import { markdown } from './markdown.js';
import { opensMark } from './savedlink.js';
import { ago, person } from './values.js';
export function list(items, ctx = null) {
    if (items.length === 0)
        return el('div', { class: 'vlist vlist-empty' }, 'Nothing yet');
    return el('div', { class: 'vlist' }, items.map((item) => row(item, ctx)));
}
function row(item, ctx) {
    const event = item.who == null;
    const meta = [
        item.who != null ? person(item.who) : null,
        item.at != null ? el('span', { class: 'vlist-at', title: item.at }, ago(item.at)) : null,
        item.chip ? chip(item.chip) : null,
    ];
    return el('div', { class: event ? 'vlist-item vlist-event' : 'vlist-item' }, event ? el('span', { class: 'vlist-dot', 'aria-hidden': 'true' }, '●') : null, el('div', { class: 'vlist-main' }, el('div', { class: 'vlist-meta' }, item.title != null && event ? titleOf(item) : null, meta, opensMark(item.opens, ctx)), item.title != null && !event ? titleOf(item) : null, item.body ? el('div', { class: 'vlist-body' }, markdown(item.body)) : null));
}
function titleOf(item) {
    return el('span', { class: 'vlist-title' }, item.title);
}
