// A timeline view block: events in order, in lanes when they name one.
import { toneClass } from '../chip.js';
import { el } from '../../kit/dom.js';
import { markdown } from './markdown.js';
import { opensMark } from '../saved/link.js';
import { ago } from '../values.js';
function event(e, lane, ctx) {
    return el('div', { class: `vtl-event ${toneClass(e.tone)}` }, 
    // `ago` hands back an `at` that is not a time as written.
    el('span', { class: 'vtl-at' }, e.at != null ? ago(e.at) : ''), el('span', { class: 'vtl-dot', 'aria-hidden': 'true' }), el('div', { class: 'vtl-main' }, lane != null && el('div', { class: 'vtl-lane-tag' }, lane), el('div', { class: 'vtl-title' }, e.title, opensMark(e.opens, ctx)), e.body && el('div', { class: 'vtl-body' }, markdown(e.body))));
}
export function timeline(block, ctx) {
    const lanes = [];
    for (const e of block.events) {
        if (e.lane != null && !lanes.includes(e.lane))
            lanes.push(e.lane);
    }
    if (lanes.length === 0) {
        return el('div', { class: 'vtl' }, block.events.map((e) => event(e, null, ctx)));
    }
    // One row per event, so row N of every lane is one step of the story and
    // the lanes read side by side. A lane with nothing at a step leaves its
    // cell empty; an event naming no lane goes in the first lane's column.
    // Narrow, the grid collapses to one column and the empty cells vanish, so
    // each event carries its lane's name.
    const rows = block.events.map((e) => {
        const own = e.lane ?? lanes[0];
        return lanes.map((lane) => lane === own ? event(e, own, ctx) : el('div', { class: 'vtl-gap', 'aria-hidden': 'true' }));
    });
    return el('div', { class: 'vtl vtl-lanes', style: `--lanes:${lanes.length}` }, lanes.map((lane) => el('div', { class: 'vtl-lane-head' }, lane)), rows);
}
