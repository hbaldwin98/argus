// Draws a sequence diagram as inline SVG from the lifelines and rows Rust
// already parsed. Text is only ever set as text nodes and every colour comes
// from CSS classes, since the strings were written by an agent.
import { el } from './kit/dom.js';
const SVG = 'http://www.w3.org/2000/svg';
const MAX_LABEL = 80;
const CHAR_W = 7;
const PAD = 20;
const HEAD_H = 30;
const ACTOR_H = 28;
const STEP_H = 44;
const SELF_H = 52;
const BRANCH_H = 32;
const SELF_W = 32;
let instances = 0;
function clip(text) {
    const chars = Array.from(text);
    return chars.length > MAX_LABEL ? chars.slice(0, MAX_LABEL - 1).join('') + '…' : text;
}
function textW(text) {
    return Array.from(clip(text)).length * CHAR_W;
}
/** Column centres, widened until the longest message between neighbours fits. */
function layout(d) {
    const cols = d.participants.map((p) => ({
        id: p.id,
        label: p.label,
        actor: p.actor,
        boxW: Math.max(90, textW(p.label) + 24),
        x: 0,
    }));
    const index = new Map(cols.map((c, i) => [c.id, i]));
    // A message may name someone the parse never declared; give them a lifeline.
    const lookup = (id) => {
        let i = index.get(id);
        if (i === undefined) {
            i = cols.length;
            cols.push({ id, label: id, actor: false, boxW: Math.max(90, textW(id) + 24), x: 0 });
            index.set(id, i);
        }
        return i;
    };
    const steps = d.rows.flatMap((r) => r.kind === 'step'
        ? [{ a: lookup(r.from), b: r.to === null ? null : lookup(r.to), t: r.text }]
        : []);
    const gaps = [];
    for (let i = 0; i + 1 < cols.length; i++) {
        gaps.push(((cols[i]?.boxW ?? 0) + (cols[i + 1]?.boxW ?? 0)) / 2 + 16);
    }
    for (const s of steps) {
        const w = textW(s.t) + 44;
        if (s.b === null) {
            if (s.a < gaps.length)
                gaps[s.a] = Math.max(gaps[s.a] ?? 0, w + SELF_W);
            continue;
        }
        const lo = Math.min(s.a, s.b);
        const hi = Math.max(s.a, s.b);
        for (let g = lo; g < hi; g++)
            gaps[g] = Math.max(gaps[g] ?? 0, w / (hi - lo));
    }
    let x = PAD + (cols[0]?.boxW ?? 0) / 2;
    cols.forEach((c, i) => {
        c.x = x;
        x += gaps[i] ?? 0;
    });
    const last = cols[cols.length - 1];
    let width = last ? last.x + last.boxW / 2 + PAD : 0;
    for (const s of steps) {
        // The last lifeline's self-call label spills past its box.
        if (s.b === null && last && cols[s.a] === last) {
            width = Math.max(width, last.x + SELF_W + 8 + textW(s.t) + PAD);
        }
    }
    for (const r of d.rows) {
        if (r.kind === 'branch')
            width = Math.max(width, textW(r.word) + textW(r.when) + 2 * PAD + 24);
    }
    return { cols, width };
}
function svgEl(tag, attrs, cls) {
    const n = document.createElementNS(SVG, tag);
    for (const [k, v] of Object.entries(attrs))
        n.setAttribute(k, String(v));
    if (cls)
        n.setAttribute('class', cls);
    return n;
}
function label(x, y, cls, content, anchor = 'middle') {
    const t = svgEl('text', { x, y, 'text-anchor': anchor }, cls);
    t.textContent = clip(content);
    if (Array.from(content).length > MAX_LABEL) {
        const title = svgEl('title', {});
        title.textContent = content;
        t.appendChild(title);
    }
    return t;
}
function marker(id, cls, open) {
    const m = svgEl('marker', {
        id,
        viewBox: '0 0 10 10',
        refX: 9,
        refY: 5,
        markerWidth: 9,
        markerHeight: 9,
        markerUnits: 'userSpaceOnUse',
        orient: 'auto',
    });
    m.appendChild(svgEl('path', { d: open ? 'M1 1L9 5L1 9' : 'M0 1L10 5L0 9z' }, cls));
    return m;
}
function header(c, y, parent) {
    parent.appendChild(svgEl('rect', { x: c.x - c.boxW / 2, y, width: c.boxW, height: HEAD_H, rx: 5 }, 'vd-box'));
    parent.appendChild(label(c.x, y + HEAD_H / 2 + 4, 'vd-name', c.label));
}
function person(cx, y, parent) {
    parent.appendChild(svgEl('circle', { cx, cy: y + 5, r: 4 }, 'vd-actor'));
    parent.appendChild(svgEl('path', {
        d: `M${cx} ${y + 9}V${y + 18}M${cx - 6} ${y + 12}H${cx + 6}M${cx - 5} ${y + 25}L${cx} ${y + 18}L${cx + 5} ${y + 25}`,
    }, 'vd-actor'));
}
function summary(d, cols) {
    const steps = d.rows.filter((r) => r.kind === 'step').length;
    return `Sequence diagram: ${steps} step${steps === 1 ? '' : 's'} between ${cols.map((c) => c.label).join(', ')}`;
}
function drawing(d) {
    const n = ++instances;
    const arrowId = `vd-arrow-${n}`;
    const openId = `vd-open-${n}`;
    const { cols, width } = layout(d);
    const byId = new Map(cols.map((c) => [c.id, c]));
    const hasActor = cols.some((c) => c.actor);
    const top = hasActor ? ACTOR_H + 4 : 4;
    const body = [];
    let y = top + HEAD_H + 12;
    for (const r of d.rows) {
        if (r.kind === 'branch') {
            const w = textW(r.word) + 16;
            body.push(svgEl('rect', { x: 4, y, width: width - 8, height: 22, rx: 3 }, 'vd-branch'));
            body.push(svgEl('rect', { x: 4, y, width: w, height: 22, rx: 3 }, 'vd-tab'));
            body.push(label(12, y + 15, 'vd-branch-word', r.word, 'start'));
            body.push(label(w + 12, y + 15, 'vd-branch-when', r.when, 'start'));
            y += BRANCH_H;
            continue;
        }
        const a = byId.get(r.from);
        if (!a)
            continue;
        const cls = r.reply ? 'vd-msg vd-reply' : 'vd-msg';
        const marked = { 'marker-end': `url(#${r.reply ? openId : arrowId})` };
        if (r.to === null) {
            const ly = y + 14;
            body.push(svgEl('path', { d: `M${a.x} ${ly}H${a.x + SELF_W}V${ly + 18}H${a.x + 2}`, ...marked }, cls));
            body.push(label(a.x + SELF_W + 8, ly + 4, 'vd-label', r.text, 'start'));
            body.push(svgEl('circle', { cx: a.x - 12, cy: ly, r: 8 }, 'vd-num-dot'));
            body.push(label(a.x - 12, ly + 3.5, 'vd-num', String(r.number)));
            y += SELF_H;
            continue;
        }
        const b = byId.get(r.to);
        if (!b)
            continue;
        const ly = y + 28;
        if (a === b) {
            y += STEP_H;
            continue;
        }
        const dir = b.x > a.x ? 1 : -1;
        body.push(svgEl('line', { x1: a.x + dir * 9, y1: ly, x2: b.x, y2: ly, ...marked }, cls));
        body.push(label((a.x + b.x) / 2, ly - 7, 'vd-label', r.text));
        body.push(svgEl('circle', { cx: a.x + dir * 9, cy: ly, r: 8 }, 'vd-num-dot'));
        body.push(label(a.x + dir * 9, ly + 3.5, 'vd-num', String(r.number)));
        y += STEP_H;
    }
    const bottom = y + 8;
    const height = bottom + HEAD_H + 8;
    const svg = svgEl('svg', {
        width,
        height,
        viewBox: `0 0 ${width} ${height}`,
        role: 'img',
        'aria-label': summary(d, cols),
    });
    const defs = svgEl('defs', {});
    defs.appendChild(marker(arrowId, 'vd-head', false));
    defs.appendChild(marker(openId, 'vd-head-open', true));
    svg.appendChild(defs);
    for (const c of cols) {
        svg.appendChild(svgEl('line', { x1: c.x, y1: top + HEAD_H, x2: c.x, y2: bottom }, 'vd-life'));
    }
    // Bands sit under the arrows but over the lifelines.
    for (const b of body)
        svg.appendChild(b);
    for (const c of cols) {
        header(c, top, svg);
        if (c.actor)
            person(c.x, 2, svg);
        header(c, bottom, svg);
    }
    return svg;
}
/** A sequence diagram drawn from its lifelines and steps. */
export function diagram(d) {
    const source = el('details', { class: 'vd-source' }, el('summary', {}, 'Source'), el('pre', {}, d.source));
    if (d.participants.length === 0 || d.rows.length === 0) {
        return el('div', { class: 'vdiagram' }, el('div', { class: 'vd-empty' }, 'Nothing to draw'), source);
    }
    return el('div', { class: 'vdiagram' }, el('div', { class: 'vd-scroll' }, drawing(d)), source);
}
