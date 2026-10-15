// A pane's screen in its container: the rows Rust sends, drawn in the
// theme's colours on canvases of its own, and the room the container has
// reported back as rows and columns. There is one screen per side Rust
// draws on: the window's canvas, which moves between the Workspace and the
// console's first side, and the console's second side while it is split.
// The rows are mirrored only so a theme or scale change can repaint them.
import { $, el, ghosts } from './dom.js';
import { call } from './tauri.js';
// The terminal's type, as the design system's --type-mono gives it.
const FONT_SIZE = 13;
const LINE_HEIGHT = 20;
// A segment's flags, as `src/page.rs` packs them.
const BOLD = 1;
const ITALIC = 2;
const UNDERLINE = 4;
const REVERSE = 8;
const WIDE = 16;
// One level of the 256-colour cube.
function step(c) {
    return c ? 55 + 40 * c : 0;
}
function clamp(n, most) {
    return Math.max(0, Math.min(most - 1, n));
}
function canvas() {
    const node = el('canvas', { 'aria-hidden': 'true' });
    const context = node.getContext('2d');
    if (!context)
        throw new Error('a canvas that draws nothing');
    return [node, context];
}
function makeScreen(terminal, side) {
    const [text, tctx] = canvas();
    const [cursorCanvas, cctx] = canvas();
    // Bars stand in for a pane until its first screen arrives.
    const waiting = el('div', { class: 'term-skeleton', hidden: true });
    waiting.replaceChildren(ghosts([[0.55], [0.8], [0.7], [0.85], [0.4], [0.75], [0.6]]));
    terminal.prepend(waiting, text, cursorCanvas);
    let pane = null;
    let rows = 0;
    let cols = 0;
    let mirror = [];
    let cursor = { row: 0, col: 0, visible: false, shape: 'block' };
    let cell = { w: 8, h: LINE_HEIGHT, ascent: 10, dpr: 1 };
    let palette = { fg: '', bg: '', ansi: [] };
    let fontFamily = 'monospace';
    // A drag across the pane, `[row, col]` at each end. Only drawn here; the
    // text it covers is read from Rust's grid when the drag ends.
    let selection = null;
    let parked = null;
    let mouse = false;
    // Whether this side has the keys: the other side of a split draws its
    // cursor hollow, as an unfocused window does.
    let keyed = true;
    function measure() {
        const css = getComputedStyle(document.documentElement);
        fontFamily = css.getPropertyValue('--font-mono').trim() || 'monospace';
        tctx.font = `${FONT_SIZE}px ${fontFamily}`;
        const m = tctx.measureText('M'.repeat(100));
        const ascent = m.fontBoundingBoxAscent || FONT_SIZE * 0.8;
        const descent = m.fontBoundingBoxDescent || FONT_SIZE * 0.2;
        cell = {
            w: m.width / 100,
            h: LINE_HEIGHT,
            ascent: (LINE_HEIGHT - (ascent + descent)) / 2 + ascent,
            dpr: window.devicePixelRatio || 1,
        };
    }
    function readPalette() {
        const css = getComputedStyle(document.documentElement);
        const v = (name) => css.getPropertyValue(name).trim();
        palette = { fg: v('--fg'), bg: v('--bg-sunken'), ansi: [] };
        for (let i = 0; i < 16; i++)
            palette.ansi.push(v(`--ansi-${i}`));
    }
    // Packed as on the wire: 0 the default, 1 << 24 | index, 2 << 24 | rgb.
    function colour(packed, fallback) {
        const kind = packed >>> 24;
        if (kind === 0)
            return fallback;
        if (kind === 2)
            return `rgb(${(packed >> 16) & 255},${(packed >> 8) & 255},${packed & 255})`;
        const i = packed & 255;
        if (i < 16)
            return palette.ansi[i] ?? fallback;
        if (i < 232) {
            const n = i - 16;
            return `rgb(${step(Math.floor(n / 36))},${step(Math.floor(n / 6) % 6)},${step(n % 6)})`;
        }
        const grey = 8 + 10 * (i - 232);
        return `rgb(${grey},${grey},${grey})`;
    }
    function size() {
        const w = Math.ceil(cols * cell.w);
        const h = rows * cell.h;
        for (const node of [text, cursorCanvas]) {
            node.width = Math.ceil(w * cell.dpr);
            node.height = Math.ceil(h * cell.dpr);
            node.style.width = `${w}px`;
            node.style.height = `${h}px`;
        }
        tctx.setTransform(cell.dpr, 0, 0, cell.dpr, 0, 0);
        cctx.setTransform(cell.dpr, 0, 0, cell.dpr, 0, 0);
        tctx.textBaseline = 'alphabetic';
    }
    function drawRow(row) {
        const y = row * cell.h;
        tctx.clearRect(0, y, cols * cell.w + 1, cell.h);
        for (const [col, cells, fg, bg, flags] of mirror[row] ?? []) {
            let fore = colour(fg, palette.fg) ?? palette.fg;
            let back = colour(bg, null);
            if (flags & REVERSE)
                [fore, back] = [back ?? palette.bg, fore];
            const x = col * cell.w;
            const width = (flags & WIDE ? 2 : cells.length) * cell.w;
            if (back) {
                tctx.fillStyle = back;
                tctx.fillRect(x, y, width, cell.h);
            }
            const italic = flags & ITALIC ? 'italic ' : '';
            const bold = flags & BOLD ? '700 ' : '';
            tctx.font = `${italic}${bold}${FONT_SIZE}px ${fontFamily}`;
            tctx.fillStyle = fore;
            if (flags & WIDE) {
                const glyph = cells[0] ?? '';
                const drawn = tctx.measureText(glyph).width;
                tctx.fillText(glyph, x + (2 * cell.w - drawn) / 2, y + cell.ascent);
            }
            else {
                cells.forEach((ch, i) => {
                    if (ch !== ' ')
                        tctx.fillText(ch, x + i * cell.w, y + cell.ascent);
                });
            }
            if (flags & UNDERLINE)
                tctx.fillRect(x, y + cell.h - 3, width, 1);
        }
    }
    function glyphAt(row, col) {
        for (const [start, cells, , , flags] of mirror[row] ?? []) {
            const span = flags & WIDE ? 2 : cells.length;
            if (col >= start && col < start + span) {
                return (flags & WIDE ? cells[0] : cells[col - start]) ?? ' ';
            }
        }
        return ' ';
    }
    function drawSelection() {
        if (!selection)
            return;
        const [start, end] = selection.anchor[0] < selection.head[0] ||
            (selection.anchor[0] === selection.head[0] && selection.anchor[1] <= selection.head[1])
            ? [selection.anchor, selection.head]
            : [selection.head, selection.anchor];
        cctx.save();
        cctx.globalAlpha = 0.3;
        cctx.fillStyle = palette.fg;
        for (let row = start[0]; row <= end[0] && row < rows; row++) {
            const from = row === start[0] ? start[1] : 0;
            const to = row === end[0] ? end[1] + 1 : cols;
            cctx.fillRect(from * cell.w, row * cell.h, (to - from) * cell.w, cell.h);
        }
        cctx.restore();
    }
    function drawCursor() {
        cctx.clearRect(0, 0, cursorCanvas.width, cursorCanvas.height);
        drawSelection();
        if (!cursor.visible || cursor.row >= rows || cursor.col >= cols)
            return;
        const x = cursor.col * cell.w;
        const y = cursor.row * cell.h;
        const focused = keyed && document.activeElement === $('input') && document.hasFocus();
        cctx.fillStyle = palette.fg;
        cctx.strokeStyle = palette.fg;
        if (cursor.shape === 'bar') {
            cctx.fillRect(x, y, 2, cell.h);
        }
        else if (cursor.shape === 'underline') {
            cctx.fillRect(x, y + cell.h - 2, cell.w, 2);
        }
        else if (!focused) {
            cctx.strokeRect(x + 0.5, y + 0.5, cell.w - 1, cell.h - 1);
        }
        else {
            cctx.fillRect(x, y, cell.w, cell.h);
            cctx.font = `${FONT_SIZE}px ${fontFamily}`;
            cctx.fillStyle = palette.bg;
            cctx.fillText(glyphAt(cursor.row, cursor.col), x, y + cell.ascent);
        }
    }
    function repaint() {
        size();
        for (let row = 0; row < rows; row++)
            drawRow(row);
        drawCursor();
    }
    function expect(next) {
        waiting.hidden = next === null || next === pane;
    }
    function frame(f) {
        waiting.hidden = true;
        // The page can run before the stylesheet applies, when every token
        // reads empty; a colour the canvas cannot parse leaves the last one
        // standing, so the palette is read again until it is there.
        if (!palette.fg)
            readPalette();
        if (f.pane !== pane || f.full) {
            if (f.pane !== pane)
                selection = null;
            pane = f.pane;
            mirror = Array.from({ length: f.rows }, () => []);
        }
        const resized = f.rows !== rows || f.cols !== cols;
        const moved = f.parked?.offset !== parked?.offset || f.parked?.depth !== parked?.depth;
        rows = f.rows;
        cols = f.cols;
        parked = f.parked;
        mouse = f.mouse;
        if (resized) {
            mirror.length = rows;
            for (let row = 0; row < rows; row++)
                mirror[row] ??= [];
            size();
        }
        // The Workspace's header names the first side's size.
        if ((resized || moved) && side === 'first') {
            const label = document.getElementById('size');
            if (label)
                label.textContent = sizeLabel();
        }
        for (const [row, segments] of f.lines)
            mirror[row] = segments;
        if (resized) {
            for (let row = 0; row < rows; row++)
                drawRow(row);
        }
        else {
            for (const [row] of f.lines)
                drawRow(row);
        }
        cursor = f.cursor;
        drawCursor();
    }
    // The cell under a point, held inside the grid so a drag past its edge
    // selects to the edge.
    function cellAt(x, y) {
        const box = text.getBoundingClientRect();
        return [
            clamp(Math.floor((y - box.top) / cell.h), rows),
            clamp(Math.floor((x - box.left) / cell.w), cols),
        ];
    }
    function select(next) {
        selection = next;
        drawCursor();
    }
    // A parked view leads with how far back it is: otherwise it looks like
    // a pane gone quiet.
    function sizeLabel() {
        if (!cols)
            return '';
        const back = parked ? `↑ ${parked.offset}/${parked.depth} · ` : '';
        return `${back}${cols} × ${rows}`;
    }
    // The room the side has, in cells. The daemon sizes a pane to the
    // client the person was last at, so the grid that comes back may differ.
    let sent = null;
    let resizeTimer;
    function report() {
        clearTimeout(resizeTimer);
        resizeTimer = setTimeout(() => {
            // A hidden terminal, or one inside another stage's hidden body, has
            // no room to report: measuring it would size the pane to one cell,
            // and the pane takes the size of the client last in use. The resize
            // observer reports again once it is laid out.
            if (terminal.hidden || !terminal.clientWidth || !terminal.clientHeight)
                return;
            const style = getComputedStyle(terminal);
            const w = terminal.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight);
            const h = terminal.clientHeight - parseFloat(style.paddingTop) - parseFloat(style.paddingBottom);
            const next = {
                rows: Math.max(1, Math.floor(h / cell.h)),
                cols: Math.max(1, Math.floor(w / cell.w)),
            };
            if (sent && sent.rows === next.rows && sent.cols === next.cols)
                return;
            sent = next;
            call('resize', { side, ...next });
        }, 50);
    }
    function rescale() {
        measure();
        readPalette();
        repaint();
        sent = null;
        report();
    }
    // A device-pixel-ratio change (a move to another monitor, a zoom) needs
    // a fresh measurement; matchMedia is how a page hears of one.
    function watchScale() {
        const query = matchMedia(`(resolution: ${window.devicePixelRatio}dppx)`);
        query.addEventListener('change', () => {
            rescale();
            watchScale();
        }, { once: true });
    }
    measure();
    readPalette();
    window.addEventListener('load', rescale);
    // A canvas never asks for a web font, and `load` does not wait for one
    // nothing has used, so the grid would be measured in the fallback face.
    const faces = ['', '700 ', 'italic ', 'italic 700 '];
    void Promise.all(faces.map((face) => document.fonts.load(`${face}${FONT_SIZE}px ${fontFamily}`)))
        .catch(() => { })
        .then(rescale);
    new ResizeObserver(report).observe(terminal);
    // A side appearing after it was hidden needs its room reported.
    new MutationObserver(report).observe(terminal, { attributes: true, attributeFilter: ['hidden'] });
    watchScale();
    matchMedia('(prefers-color-scheme: light)').addEventListener('change', () => {
        readPalette();
        repaint();
    });
    // Whether the child asked for the mouse: a press is then its, and only
    // Shift-drag selects here.
    function takesMouse() {
        return mouse && !parked;
    }
    /** The theme changed under the canvas: its colours are read again. */
    function retheme() {
        readPalette();
        repaint();
    }
    /** Whether this side has the keys, which its cursor shows. */
    function key(on) {
        if (keyed === on)
            return;
        keyed = on;
        drawCursor();
    }
    return {
        terminal,
        pane: () => pane,
        frame,
        drawCursor,
        report,
        sizeLabel,
        cellAt,
        select,
        takesMouse,
        expect,
        retheme,
        key,
    };
}
/** The window's canvas: the Workspace's pane, or the console's first side. */
export const screen = makeScreen($('terminal'), 'first');
/** The console's second side, while it is split. */
export const beside = makeScreen($('terminal-beside'), 'second');
export const screens = [screen, beside];
/** A frame goes to the side Rust says draws it. */
export function drawFrame(f) {
    (f.side === 'second' ? beside : screen).frame(f);
}
