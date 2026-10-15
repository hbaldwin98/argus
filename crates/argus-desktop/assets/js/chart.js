// A chart view block: series over a shared x axis, drawn as inline SVG. The
// axes, the scale and the thinning of labels are the page's; an agent sends
// the data and a tone per series, never a colour.
import { toneClass } from './chip.js';
import { el } from './dom.js';
const SVG = 'http://www.w3.org/2000/svg';
const W = 480;
const H = 160;
const PAD = { left: 44, right: 10, top: 10, bottom: 24 };
// Series of one tone cannot be told apart by hue, so they differ in stroke.
const DASHES = ['', '6 3', '2 3', '8 3 2 3'];
const FILLS = [1, 0.6, 0.35, 0.8];
const LABEL_CHAR_PX = 6;
function svg(tag, attrs, ...kids) {
    const node = document.createElementNS(SVG, tag);
    for (const [k, v] of Object.entries(attrs))
        node.setAttribute(k, String(v));
    for (const kid of kids)
        node.appendChild(kid);
    return node;
}
function title(text) {
    const node = document.createElementNS(SVG, 'title');
    node.textContent = text;
    return node;
}
function svgText(attrs, content) {
    const node = svg('text', attrs);
    node.textContent = content;
    return node;
}
function compact(n) {
    return n.toLocaleString('en-US', { notation: 'compact', maximumFractionDigits: 2 });
}
function niceStep(raw) {
    const pow = 10 ** Math.floor(Math.log10(raw));
    const f = raw / pow;
    return (f <= 1 ? 1 : f <= 2 ? 2 : f <= 5 ? 5 : 10) * pow;
}
/** Round ticks from at most zero to at least the data's top, 3 to 5 of them. */
function scale(min, max) {
    let lo = Math.min(0, min);
    let hi = Math.max(max, lo);
    if (hi - lo < 1e-9)
        hi = lo + 1;
    const step = niceStep((hi - lo) / 3);
    lo = Math.floor(lo / step + 1e-9) * step;
    hi = Math.ceil(hi / step - 1e-9) * step;
    const ticks = [];
    for (let t = lo; t <= hi + step / 2; t += step)
        ticks.push(Number(t.toPrecision(12)));
    return { lo, hi, ticks };
}
function finite(v) {
    return v != null && Number.isFinite(v);
}
/** The runs of consecutive present values, so a gap breaks the line. */
function runs(points) {
    const out = [];
    let cur = [];
    for (const p of points) {
        if (p)
            cur.push(p);
        else if (cur.length) {
            out.push(cur);
            cur = [];
        }
    }
    if (cur.length)
        out.push(cur);
    return out;
}
const path = (pts) => pts.map((p, i) => `${i ? 'L' : 'M'}${p.x.toFixed(1)} ${p.y.toFixed(1)}`).join('');
/** How many series before this one share its tone: which stroke it takes. */
function twinIndex(series, i) {
    return series.slice(0, i).filter((s) => s.tone === series[i].tone).length;
}
function dashed(twin) {
    const dash = DASHES[twin % DASHES.length];
    return dash ? { 'stroke-dasharray': dash } : {};
}
function summary(block) {
    const names = block.series.map((s) => s.name).join(', ');
    const all = block.series.flatMap((s) => s.values).filter(finite);
    const unit = block.unit ? ` ${block.unit}` : '';
    const range = all.length
        ? `, from ${compact(Math.min(...all))} to ${compact(Math.max(...all))}${unit}`
        : '';
    return `${block.shape} chart of ${names || 'no series'} over ${block.x.length} points${range}`;
}
function legend(series) {
    return el('div', { class: 'vchart-legend' }, series.map((s, i) => {
        const key = svg('svg', { width: 22, height: 8, viewBox: '0 0 22 8', 'aria-hidden': 'true' }, svg('line', {
            x1: 1,
            y1: 4,
            x2: 21,
            y2: 4,
            class: 'vchart-key',
            ...dashed(twinIndex(series, i)),
        }));
        return el('span', { class: `vchart-name ${toneClass(s.tone)}` }, key, s.name);
    }));
}
function axes(block, lo, hi, ticks, bandX) {
    const plotH = H - PAD.top - PAD.bottom;
    const y = (v) => PAD.top + plotH * (1 - (v - lo) / (hi - lo));
    const g = svg('g', { class: 'vchart-axes' });
    for (const t of ticks) {
        g.appendChild(svg('line', { x1: PAD.left, x2: W - PAD.right, y1: y(t), y2: y(t), class: 'vchart-grid' }));
        g.appendChild(svgText({ x: PAD.left - 6, y: y(t) + 3.5, 'text-anchor': 'end' }, compact(t)));
    }
    const longest = Math.max(1, ...block.x.map((s) => Array.from(s).length));
    const slot = (W - PAD.left - PAD.right) / Math.max(block.x.length, 1);
    const every = Math.max(1, Math.ceil((longest * LABEL_CHAR_PX + 10) / slot));
    block.x.forEach((label, i) => {
        if (i % every)
            return;
        g.appendChild(svgText({ x: bandX(i), y: H - PAD.bottom + 14, 'text-anchor': 'middle' }, label));
    });
    return { g, y };
}
function drawChart(block) {
    const n = block.x.length;
    const present = block.series.flatMap((s) => s.values.slice(0, n)).filter(finite);
    const { lo, hi, ticks } = scale(Math.min(...present, 0), Math.max(...present, 0));
    const plotW = W - PAD.left - PAD.right;
    const bars = block.shape === 'bar';
    // Bars sit in bands; a line runs edge to edge, or centred if it is one point.
    const bandX = (i) => bars || n < 2
        ? PAD.left + (plotW * (i + 0.5)) / Math.max(n, 1)
        : PAD.left + (plotW * i) / (n - 1);
    const { g: axesG, y } = axes(block, lo, hi, ticks, bandX);
    const root = svg('svg', {
        viewBox: `0 0 ${W} ${H}`,
        class: 'vchart-svg',
        role: 'img',
        'aria-label': summary(block),
    });
    root.appendChild(axesG);
    const unit = block.unit ? ` ${block.unit}` : '';
    const hover = (i, v) => `${block.x[i] ?? ''}: ${v.toLocaleString()}${unit}`;
    const groupW = (plotW / Math.max(n, 1)) * 0.7;
    const barW = groupW / Math.max(block.series.length, 1);
    block.series.forEach((s, si) => {
        const twin = twinIndex(block.series, si);
        const fill = FILLS[twin % FILLS.length];
        const layer = svg('g', { class: toneClass(s.tone) });
        const values = s.values.slice(0, n);
        if (bars) {
            values.forEach((v, i) => {
                if (!finite(v))
                    return;
                layer.appendChild(svg('rect', {
                    x: bandX(i) - groupW / 2 + barW * si + 0.5,
                    y: Math.min(y(v), y(0)),
                    width: Math.max(barW - 1, 1),
                    height: Math.max(Math.abs(y(v) - y(0)), 1),
                    class: 'vchart-bar',
                    'fill-opacity': fill,
                }, title(hover(i, v))));
            });
        }
        else {
            for (const run of runs(values.map((v, i) => (finite(v) ? { x: bandX(i), y: y(v) } : null)))) {
                if (run.length < 2)
                    continue;
                if (block.shape === 'area') {
                    const a = run[0];
                    const b = run[run.length - 1];
                    const base = y(0).toFixed(1);
                    layer.appendChild(svg('path', {
                        d: `${path(run)}L${b.x.toFixed(1)} ${base}L${a.x.toFixed(1)} ${base}Z`,
                        class: 'vchart-area',
                        'fill-opacity': 0.18 * fill,
                    }));
                }
                layer.appendChild(svg('path', { d: path(run), class: 'vchart-line', ...dashed(twin) }));
            }
            values.forEach((v, i) => {
                if (!finite(v))
                    return;
                layer.appendChild(svg('circle', { cx: bandX(i), cy: y(v), r: 3, class: 'vchart-dot' }, title(hover(i, v))));
            });
        }
        root.appendChild(layer);
    });
    return root;
}
/** A small line with no axes, to sit inline: a stat's trend, a spark chart. */
export function spark(values, tone) {
    const vals = values.filter(Number.isFinite);
    const lo = Math.min(...vals);
    const hi = Math.max(...vals);
    const pts = vals.map((v, i) => ({
        x: vals.length < 2 ? 50 : (100 * i) / (vals.length - 1),
        y: hi === lo ? 16 : 28 - (24 * (v - lo)) / (hi - lo),
    }));
    const root = svg('svg', {
        viewBox: '0 0 100 32',
        // Stretched to the tile; the stroke stays the same width (see CSS).
        preserveAspectRatio: 'none',
        class: `vspark ${toneClass(tone)}`,
        role: 'img',
        'aria-label': vals.length
            ? `trend over ${vals.length} points, from ${compact(lo)} to ${compact(hi)}`
            : 'no trend',
    });
    if (pts.length > 1)
        root.appendChild(svg('path', { d: path(pts), class: 'vchart-line' }));
    return root;
}
export function chart(block) {
    if (block.shape === 'spark') {
        const first = block.series[0];
        return el('div', { class: 'vchart vchart-spark' }, spark((first?.values ?? []).filter(finite), first?.tone ?? 'neutral'));
    }
    return el('div', { class: 'vchart' }, drawChart(block), block.series.length > 1 && legend(block.series));
}
