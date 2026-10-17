// A progress view block, and the meter a progress value draws in a cell.
import { toneClass } from './chip.js';
import { el } from './kit/dom.js';
/** How far along, 0 to 1; an agent's overshoot or a zero max is not a broken bar. */
function fraction(value, max) {
    if (!(max > 0) || !Number.isFinite(value))
        return 0;
    return Math.min(Math.max(value / max, 0), 1);
}
function meter(frac, tone, cls) {
    const fill = el('span', { class: 'vmeter-fill' });
    fill.style.width = `${frac * 100}%`;
    return el('span', { class: `${cls} ${toneClass(tone)}` }, fill);
}
/** A compact inline meter with the percentage, for a cell or a field. */
export function progressValue(v) {
    const frac = fraction(v.value, v.max);
    const pct = `${Math.round(frac * 100)}%`;
    return el('span', {
        class: 'vprogress-inline',
        role: 'progressbar',
        'aria-valuemin': 0,
        'aria-valuemax': v.max,
        'aria-valuenow': frac * v.max,
        'aria-valuetext': pct,
    }, meter(frac, v.tone, 'vmeter vmeter-sm'), el('span', { class: 'vnum' }, pct));
}
export function progress(block) {
    const frac = fraction(block.value, block.max);
    return el('div', {
        class: 'vprogress',
        role: 'progressbar',
        'aria-label': block.label,
        'aria-valuemin': 0,
        'aria-valuemax': block.max,
        'aria-valuenow': frac * block.max,
        'aria-valuetext': block.note ?? `${Math.round(frac * 100)}%`,
    }, el('div', { class: 'vprogress-head' }, el('span', null, block.label), block.note != null && el('span', { class: 'vprogress-note' }, block.note)), meter(frac, block.tone, 'vmeter'));
}
