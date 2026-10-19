// A drawing a view block makes — a graph, a sequence diagram — that opens
// larger over the window when clicked, to be zoomed with the wheel and
// dragged, with its source beside it when it has one. The larger view is
// the page's alone, like a chosen tab: it is drawn again from the block,
// so its listeners work, and nothing about it reaches Rust. While it is
// up it holds the keys, as a modal does.
import { actionFor } from '../../keys/keymap.js';
import { el, icon } from '../../kit/dom.js';
import { fit, start, zoomAt } from './zoom.js';
/** A pointer that moves less than this was a click, not a drag. */
const DRAG = 4;
const STEP = 1.25;
/** `inline` with a button that opens it larger; a click on it does too,
 *  unless it lands on something of its own to press. */
export function expandable(inline, figure) {
    inline.addEventListener('click', (e) => {
        if (e.target instanceof Element && e.target.closest('.vg-opens, a, button, summary'))
            return;
        if (window.getSelection()?.toString())
            return;
        open(figure);
    });
    inline.classList.add('vfig-inline');
    return el('div', { class: 'vfig' }, inline, el('button', {
        class: 'icon vfig-open',
        type: 'button',
        title: 'Open larger',
        'aria-label': `Open ${figure.title} larger`,
        onclick: () => open(figure),
    }, icon('expand')));
}
let showing = null;
function open(figure) {
    showing?.();
    const back = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const svg = figure.draw();
    const box = svg instanceof SVGSVGElement ? svg.viewBox.baseVal : null;
    const [w, h] = box && box.width > 0 ? [box.width, box.height] : [400, 300];
    svg.removeAttribute('style');
    const canvas = el('div', { class: 'vfig-canvas' });
    canvas.appendChild(svg);
    const stage = el('div', { class: 'vfig-stage' }, canvas);
    const pct = el('button', {
        class: 'segb vfig-pct',
        type: 'button',
        title: 'Actual size',
        onclick: () => set(zoomAt(zoom, 1 / zoom.scale, ...middle())),
    });
    let zoom = { scale: 1, x: 0, y: 0 };
    const set = (z) => {
        zoom = z;
        svg.setAttribute('width', String(w * z.scale));
        svg.setAttribute('height', String(h * z.scale));
        canvas.style.transform = `translate(${z.x}px, ${z.y}px)`;
        pct.textContent = `${Math.round(z.scale * 100)}%`;
    };
    const middle = () => [stage.clientWidth / 2, stage.clientHeight / 2];
    const by = (factor) => set(zoomAt(zoom, factor, ...middle()));
    const whole = () => set(fit(w, h, stage.clientWidth, stage.clientHeight));
    let sourceShown = false;
    const source = figure.source !== undefined && el('pre', { class: 'vfig-source', hidden: true }, figure.source);
    const sourceButton = source &&
        el('button', {
            class: 'btn sm',
            type: 'button',
            'aria-pressed': 'false',
            onclick: (e) => {
                const button = e.currentTarget;
                sourceShown = !sourceShown;
                source.hidden = !sourceShown;
                button.classList.toggle('on', sourceShown);
                button.setAttribute('aria-pressed', String(sourceShown));
            },
        }, 'Source');
    const frame = el('div', {
        class: 'vfig-frame',
        role: 'dialog',
        'aria-modal': 'true',
        'aria-label': figure.title,
        tabindex: '-1',
    }, el('div', { class: 'vfig-bar' }, el('span', { class: 'type-heading grow ttl', title: figure.title }, figure.title), el('span', { class: 'meta vfig-hint' }, 'Wheel to zoom · drag to move'), el('div', { class: 'seg', role: 'group', 'aria-label': 'Zoom' }, el('button', { class: 'segb', type: 'button', 'aria-label': 'Zoom out', onclick: () => by(1 / STEP) }, '−'), pct, el('button', { class: 'segb', type: 'button', 'aria-label': 'Zoom in', onclick: () => by(STEP) }, '+')), el('button', { class: 'btn sm', type: 'button', onclick: whole }, 'Fit'), sourceButton, el('button', {
        class: 'icon',
        type: 'button',
        title: 'Close',
        'aria-label': 'Close',
        onclick: () => close(),
    }, icon('close'))), el('div', { class: 'vfig-body' }, stage, source));
    const over = el('div', {
        class: 'vfig-over',
        onmousedown: (e) => {
            if (e.target === e.currentTarget)
                close();
        },
    }, frame);
    stage.addEventListener('wheel', (e) => {
        e.preventDefault();
        const r = stage.getBoundingClientRect();
        const lines = e.deltaMode === 1 ? 16 : e.deltaMode === 2 ? r.height : 1;
        set(zoomAt(zoom, Math.exp(-e.deltaY * lines * 0.0015), e.clientX - r.left, e.clientY - r.top));
    }, { passive: false });
    // A drag moves the drawing; a press that hardly moved stays a click, so
    // a box that opens a view still opens it.
    let press = null;
    let swallow = false;
    stage.addEventListener('pointerdown', (e) => {
        if (e.button !== 0)
            return;
        press = { x: e.clientX, y: e.clientY, at: zoom, moved: false };
    });
    stage.addEventListener('pointermove', (e) => {
        if (!press)
            return;
        const dx = e.clientX - press.x;
        const dy = e.clientY - press.y;
        if (!press.moved && Math.hypot(dx, dy) < DRAG)
            return;
        if (!press.moved) {
            press.moved = true;
            stage.setPointerCapture(e.pointerId);
            stage.classList.add('dragging');
        }
        set({ ...press.at, x: press.at.x + dx, y: press.at.y + dy });
    });
    const release = () => {
        swallow = press?.moved ?? false;
        press = null;
        stage.classList.remove('dragging');
    };
    stage.addEventListener('pointerup', release);
    stage.addEventListener('pointercancel', release);
    stage.addEventListener('click', (e) => {
        if (!swallow)
            return;
        swallow = false;
        e.stopPropagation();
        e.preventDefault();
    }, true);
    // A box that opened a view opened it behind this; show it.
    stage.addEventListener('click', (e) => {
        if (e.target instanceof Element && e.target.closest('.vg-opens'))
            close();
    });
    const keys = (e) => {
        if (!over.isConnected)
            return;
        e.stopPropagation();
        // The flows lens's diagram keys, so the sheet already lists them.
        const action = actionFor(['diagram', 'anywhere'], e);
        if (action === 'closeLayer')
            close();
        else if (action === 'zoomIn')
            by(STEP);
        else if (action === 'zoomOut')
            by(1 / STEP);
        else if (action === 'zoomReset')
            set(zoomAt(zoom, 1 / zoom.scale, ...middle()));
        else if (action === 'fit')
            whole();
        else
            return;
        e.preventDefault();
    };
    const close = () => {
        window.removeEventListener('keydown', keys, true);
        over.remove();
        showing = null;
        back?.focus();
    };
    showing = close;
    window.addEventListener('keydown', keys, true);
    document.body.appendChild(over);
    set(start(w, h, stage.clientWidth, stage.clientHeight));
    frame.focus();
}
