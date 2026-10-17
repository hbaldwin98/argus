// A tinted note in the view's tone. The icons are built as nodes, not
// markup, because the shared icon table has no warning or info glyphs.
import { toneClass } from '../chip.js';
import { el } from '../../kit/dom.js';
const SVG = 'http://www.w3.org/2000/svg';
const GLYPH = {
    warn: ['M8 2.5L14 13H2z', 'M8 7v3M8 11.8v.1'],
    bad: ['M8 2.5a5.5 5.5 0 1 0 0 11 5.5 5.5 0 0 0 0-11z', 'M5.9 5.9l4.2 4.2M10.1 5.9l-4.2 4.2'],
    good: ['M8 2.5a5.5 5.5 0 1 0 0 11 5.5 5.5 0 0 0 0-11z', 'M5.5 8.2l1.8 1.8 3.2-3.6'],
    info: ['M8 2.5a5.5 5.5 0 1 0 0 11 5.5 5.5 0 0 0 0-11z', 'M8 7.4v3.2M8 5.2v.1'],
    neutral: [],
    active: [],
};
function glyph(tone) {
    const svg = document.createElementNS(SVG, 'svg');
    for (const [k, v] of Object.entries({
        width: '14',
        height: '14',
        viewBox: '0 0 16 16',
        fill: 'none',
        stroke: 'currentColor',
        'stroke-width': '1.5',
        'stroke-linecap': 'round',
        'stroke-linejoin': 'round',
        'aria-hidden': 'true',
    })) {
        svg.setAttribute(k, v);
    }
    const paths = GLYPH[tone].length ? GLYPH[tone] : GLYPH.info;
    for (const d of paths) {
        const path = document.createElementNS(SVG, 'path');
        path.setAttribute('d', d);
        svg.appendChild(path);
    }
    return svg;
}
export function callout(tone, text) {
    return el('div', { class: `vcallout ${toneClass(tone)}`, role: 'note' }, glyph(tone), el('span', { class: 'vcallout-text' }, text));
}
