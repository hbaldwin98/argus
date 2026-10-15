// A graph view block: boxes and arrows the page lays out. `graph_layout.ts`
// decides where everything goes; this draws it as inline SVG. Labels are
// text nodes and every colour is a class, since the strings were written
// by an agent.
import { toneClass } from './chip.js';
import { el } from './dom.js';
import { clip, layout } from './graph_layout.js';
const SVG = 'http://www.w3.org/2000/svg';
const NAMED = 6;
let instances = 0;
function svgEl(tag, attrs, cls) {
    const n = document.createElementNS(SVG, tag);
    for (const [k, v] of Object.entries(attrs))
        n.setAttribute(k, String(v));
    if (cls)
        n.setAttribute('class', cls);
    return n;
}
function text(x, y, label, cls) {
    const t = svgEl('text', { x, y, 'text-anchor': 'middle', 'dominant-baseline': 'central' }, cls);
    t.textContent = clip(label);
    if (clip(label) !== label) {
        const title = svgEl('title', {});
        title.textContent = label;
        t.appendChild(title);
    }
    return t;
}
/** What a screen reader hears of it: the size, then the first few links. */
function describe(block) {
    const name = new Map(block.nodes.map((n) => [n.id, n.label]));
    const links = block.edges
        .slice(0, NAMED)
        .map((e) => `${name.get(e.from) ?? e.from} to ${name.get(e.to) ?? e.to}`)
        .join(', ');
    const more = block.edges.length > NAMED ? ', and more' : '';
    return `Graph of ${block.nodes.length} nodes and ${block.edges.length} edges${links ? `: ${links}${more}` : ''}`;
}
export function graph(block) {
    if (block.nodes.length === 0) {
        return el('div', { class: 'vg' }, el('div', { class: 'vg-empty' }, 'Nothing to draw'));
    }
    const laid = layout(block.nodes, block.edges, block.direction === 'right');
    const marker = `vg-head-${instances++}`;
    const svg = svgEl('svg', {
        viewBox: `0 0 ${laid.width} ${laid.height}`,
        width: laid.width,
        role: 'img',
        'aria-label': describe(block),
    });
    // Scaled down to the panel but never up past the size it was laid out at.
    svg.setAttribute('style', `max-width:${laid.width}px`);
    const defs = svgEl('defs', {});
    const head = svgEl('marker', {
        id: marker,
        viewBox: '0 0 10 10',
        refX: 9,
        refY: 5,
        markerWidth: 8,
        markerHeight: 8,
        orient: 'auto',
    });
    head.appendChild(svgEl('path', { d: 'M0 1L10 5L0 9z' }, 'vg-head'));
    defs.appendChild(head);
    svg.appendChild(defs);
    block.edges.forEach((e, i) => {
        const route = laid.routes[i];
        if (!route)
            return;
        const d = route.points.map(([x, y], k) => `${k === 0 ? 'M' : 'L'}${x} ${y}`).join('');
        svg.appendChild(svgEl('path', { d, 'marker-end': `url(#${marker})` }, route.loop ? 'vg-edge vg-loop' : 'vg-edge'));
        if (e.label)
            svg.appendChild(text(route.mid[0], route.mid[1], e.label, 'vg-elabel'));
    });
    block.nodes.forEach((n, i) => {
        const p = laid.nodes[i];
        if (!p)
            return;
        const g = svgEl('g', {}, `vg-node ${toneClass(n.tone)}`);
        g.appendChild(svgEl('rect', { x: p.x, y: p.y, width: p.w, height: p.h, rx: 6 }, 'vg-box'));
        g.appendChild(text(p.x + p.w / 2, p.y + p.h / 2, n.label, 'vg-name'));
        svg.appendChild(g);
    });
    return el('div', { class: 'vg' }, svg);
}
