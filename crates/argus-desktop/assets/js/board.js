// A board view block: columns of cards.
import { chip } from './chip.js';
import { el } from './kit/dom.js';
import { markdown } from './markdown.js';
import { opensMark } from './savedlink.js';
import { value } from './values.js';
function card(c, ctx) {
    return el('div', { class: 'vboard-card' }, el('div', { class: 'vboard-card-head' }, el('span', { class: 'vboard-card-title' }, c.title), opensMark(c.opens, ctx), c.chip && chip(c.chip)), c.value && el('div', { class: 'vboard-card-value' }, value(c.value, ctx)), c.body && el('div', { class: 'vboard-card-body' }, markdown(c.body)));
}
export function board(block, ctx) {
    return el('div', { class: 'vboard' }, block.columns.map((column) => el('section', { class: 'vboard-col' }, el('header', { class: 'vboard-col-head' }, el('span', { class: 'vboard-col-title' }, column.title), el('span', { class: 'vboard-count' }, column.cards.length), column.chip && chip(column.chip)), el('div', { class: 'vboard-cards' }, column.cards.map((c) => card(c, ctx))))));
}
