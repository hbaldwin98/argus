// A stats view block: tiles of an already-formatted value under a label, with
// an optional delta coloured by the stat's tone, and a spark of its trend.
import { toneClass } from './chip.js';
import { spark } from './chart.js';
import { el } from './dom.js';
import { opensMark } from './savedlink.js';
export function stats(list, ctx) {
    return el('div', { class: 'vstats' }, list.map((s) => el('div', { class: `vstat ${toneClass(s.tone)}` }, el('span', { class: 'vstat-label' }, s.label, opensMark(s.opens, ctx)), el('span', { class: 'vstat-value' }, s.value), s.delta != null
        ? el('span', { class: s.tone === 'neutral' ? 'vstat-delta muted' : 'vstat-delta' }, s.delta)
        : null, s.trend.length > 0 ? spark(s.trend, s.tone) : null)));
}
