// A flow's sequence diagram, drawn from what Rust read of its source
// (`src/board/lenses/sequence.rs`): lifelines left to right, calls
// solid, replies dashed, a lifeline's call to itself looped back, and the
// frames an `alt`, `opt` or `loop` draws, the selected step lit. The Flows
// lens draws it beside the steps; expanded, it fills the window with a
// toolbar and the steps beside it, and a step's arrow dragged up or down,
// or a lifeline dragged sideways, is sent as the flow's source rewritten —
// the diagram moves when the daemon's pushed flows say it did.
import { el } from '../../kit/dom.js';
import { bindMenu, flowSteps, selectStep } from './flows.js';
import { state } from '../../state.js';
import { call } from '../../connection/tauri.js';
import { begin, drawWork, edit, field, formButtons, keycap, lineList, titled } from './work.js';
const NS = 'http://www.w3.org/2000/svg';
const MARGIN = 28;
const COLUMN = 176;
const HEAD = 34;
const STEP = 46;
const SELF = 60;
function svg(tag, attrs, ...children) {
    const node = document.createElementNS(NS, tag);
    for (const [key, value] of Object.entries(attrs))
        node.setAttribute(key, String(value));
    for (const child of children)
        node.append(child);
    return node;
}
export function drawSequence(sequence) {
    const count = Math.max(sequence.lifelines.length, 1);
    // A call to itself on the last lifeline writes its text out to the right.
    const loops = sequence.items.some((item) => item.kind === 'message' && item.from === item.to && item.from === count - 1);
    // A self-call's text starts 48 right of its lifeline; the frame must reach
    // past its end, measured at 12px mono (7.2 a character) plus a margin.
    const reach = sequence.items.reduce((most, item) => {
        if (item.kind !== 'message' || item.from !== item.to)
            return most;
        return Math.max(most, MARGIN + COLUMN / 2 + item.from * COLUMN + 48 + item.text.length * 7.2);
    }, 0);
    const width = Math.max(MARGIN * 2 + count * COLUMN + (loops ? 120 : 0), Math.ceil(reach) + MARGIN + 8);
    const x = (at) => MARGIN + COLUMN / 2 + at * COLUMN;
    const frames = svg('g', { class: 'frames' });
    const messages = svg('g', { class: 'messages' });
    const steps = [];
    const open = [];
    let y = MARGIN + HEAD + 30;
    for (const item of sequence.items) {
        switch (item.kind) {
            case 'message': {
                const from = x(item.from);
                const lit = item.on ? ' on' : '';
                const named = (to) => sequence.lifelines[to]?.label ?? '';
                const group = svg('g', {
                    class: `smsg${lit}${item.reply ? ' reply' : ''}`,
                    'data-step': item.step,
                    role: 'button',
                    tabindex: 0,
                    'aria-label': `Step ${item.step}: ${named(item.from)} → ${named(item.to)}, ${item.text}`,
                });
                if (item.from === item.to) {
                    const loop = `M ${from} ${y} H ${from + 40} V ${y + 22} H ${from + 4}`;
                    group.append(svg('path', { class: 'hit', d: loop }), svg('path', { class: 'line', d: loop }), arrowhead(from + 4, y + 22, -1, item.reply), svg('text', { class: 'mtext', x: from + 48, y: y + 15 }, item.text));
                    steps.push({ step: item.step, y: y + 11 });
                    badge(group, from, y, item.step);
                    y += SELF;
                }
                else {
                    const to = x(item.to);
                    const dir = Math.sign(to - from);
                    group.append(svg('line', { class: 'hit', x1: from, y1: y, x2: to, y2: y }), svg('line', { class: 'line', x1: from, y1: y, x2: to - dir * 2, y2: y }), arrowhead(to, y, dir, item.reply), svg('text', { class: 'mtext', x: (from + to) / 2, y: y - 8, 'text-anchor': 'middle' }, item.text));
                    steps.push({ step: item.step, y });
                    badge(group, from, y, item.step);
                    y += STEP;
                }
                messages.append(group);
                break;
            }
            case 'open':
                open.push({ kind: item.frame, label: item.label, top: y - 22, arms: [] });
                y += 22;
                break;
            case 'arm': {
                const frame = open.at(-1);
                if (frame) {
                    frame.arms.push(y - 18);
                    messages.append(svg('text', { class: 'ftext', x: MARGIN + 14, y: y - 4 }, `[${item.label}]`));
                }
                y += 20;
                break;
            }
            case 'close': {
                const frame = open.pop();
                if (!frame)
                    break;
                const inset = 6 + open.length * 8;
                const left = MARGIN + inset;
                const right = width - MARGIN - inset;
                const bottom = y - 16;
                frames.append(svg('rect', {
                    class: 'frame',
                    x: left,
                    y: frame.top,
                    width: right - left,
                    height: bottom - frame.top,
                    rx: 4,
                }), svg('path', {
                    class: 'ftab',
                    d: `M ${left} ${frame.top + 18} H ${left + 40} L ${left + 48} ${frame.top + 10} V ${frame.top} H ${left}`,
                }), svg('text', { class: 'fkind', x: left + 6, y: frame.top + 13 }, frame.kind), svg('text', { class: 'ftext', x: left + 56, y: frame.top + 13 }, `[${frame.label}]`), ...frame.arms.map((at) => svg('line', { class: 'farm', x1: left, y1: at, x2: right, y2: at })));
                y += 6;
                break;
            }
        }
    }
    const height = y + MARGIN;
    const lines = svg('g', { class: 'lifelines' });
    const lifelines = [];
    sequence.lifelines.forEach((lifeline, at) => {
        const cx = x(at);
        lifelines.push(cx);
        const w = COLUMN - 28;
        lines.append(svg('line', { class: 'life', x1: cx, y1: MARGIN + HEAD, x2: cx, y2: height - MARGIN }), svg('g', { class: 'lhead', 'data-lifeline': at }, svg('rect', { x: cx - w / 2, y: MARGIN, width: w, height: HEAD, rx: 6 }), svg('text', { x: cx, y: MARGIN + HEAD / 2 + 4, 'text-anchor': 'middle' }, lifeline.label)));
    });
    const node = svg('svg', { class: 'seq', viewBox: `0 0 ${width} ${height}`, width, height, role: 'img' }, frames, lines, messages);
    return { node, width, height, steps, lifelines };
}
function arrowhead(x, y, dir, reply) {
    const back = x - dir * 9;
    return reply
        ? svg('path', { class: 'head-open', d: `M ${back} ${y - 5} L ${x} ${y} L ${back} ${y + 5}` })
        : svg('path', {
            class: 'head-solid',
            d: `M ${back} ${y - 5} L ${x} ${y} L ${back} ${y + 5} Z`,
        });
}
function badge(group, x, y, step) {
    group.append(svg('circle', { class: 'sbadge', cx: x, cy: y, r: 9 }), svg('text', { class: 'btext', x, y: y + 3.5, 'text-anchor': 'middle' }, String(step)));
}
/** Clicks on a drawn diagram's arrows put the cursor on their step. */
export function clickable(drawn, flow) {
    drawn.node.setAttribute('aria-label', `Sequence diagram: ${flow.title}`);
    drawn.node.addEventListener('click', (e) => {
        const step = stepOf(e.target);
        if (step !== null && !drag.moved)
            selectStep(flow.id, step - 1);
    });
    drawn.node.addEventListener('keydown', (e) => {
        // `code`, not the key's name: the keymap owns what a key means, and these
        // are only the keys that press a focused control, as a button's own do.
        if (e.code !== 'Enter' && e.code !== 'Space' && e.code !== 'NumpadEnter')
            return;
        const step = stepOf(e.target);
        if (step === null)
            return;
        e.preventDefault();
        selectStep(flow.id, step - 1);
    });
}
function stepOf(target) {
    if (!(target instanceof Element))
        return null;
    const at = target.closest('[data-step]')?.getAttribute('data-step');
    return at ? Number(at) : null;
}
// ---- Expanded ---------------------------------------------------------------
export function expandedOpen() {
    return state.expanded !== null && state.features?.open?.lens === 'flows';
}
export function openExpanded() {
    state.expanded ??= { zoom: 1, fit: true, panel: true };
    drawExpanded();
}
export function closeExpanded() {
    state.expanded = null;
    document.getElementById('expanded')?.remove();
}
/** Draws the expanded diagram over the window, or takes it away when the
 *  flow it was showing is no longer on screen. */
export function drawExpanded() {
    const view = state.features?.open;
    const flow = view?.flows?.flow;
    const expanded = state.expanded;
    if (!expanded || !view || view.lens !== 'flows' || !flow) {
        if (expanded && view?.lens === 'flows' && view.flows?.loaded === false)
            return;
        closeExpanded();
        return;
    }
    const canvas = el('div', { class: 'expanded-canvas' });
    const drawn = drawSequence(flow.diagram);
    clickable(drawn, flow);
    draggable(drawn, flow, canvas);
    canvas.append(drawn.node);
    const node = el('div', { class: 'expanded', id: 'expanded', role: 'dialog', 'aria-label': flow.title }, toolbar(view), el('div', { class: 'expanded-body' }, canvas, expanded.panel && panel(view, flow)));
    const old = document.getElementById('expanded');
    const scroll = old?.querySelector('.expanded-canvas');
    if (old)
        old.replaceWith(node);
    else
        document.getElementById('layer')?.before(node);
    // Measured once it is on the page: fit is the canvas's size over the
    // diagram's.
    if (expanded.fit) {
        const box = canvas.getBoundingClientRect();
        const fits = Math.min((box.width - 32) / drawn.width, (box.height - 32) / drawn.height, 2);
        expanded.zoom = Math.max(0.25, Math.round(fits * 100) / 100);
        const zoom = node.querySelector('.zoom-pct');
        if (zoom)
            zoom.textContent = `${Math.round(expanded.zoom * 100)}%`;
    }
    drawn.node.setAttribute('width', String(drawn.width * expanded.zoom));
    drawn.node.setAttribute('height', String(drawn.height * expanded.zoom));
    if (scroll) {
        canvas.scrollLeft = scroll.scrollLeft;
        canvas.scrollTop = scroll.scrollTop;
    }
    canvas.querySelector('.smsg.on')?.scrollIntoView({ block: 'nearest', inline: 'nearest' });
}
function toolbar(view) {
    const expanded = state.expanded;
    const flows = view.flows?.flows ?? [];
    const switcher = el('select', {
        class: 'field flow-switch',
        'aria-label': 'Flow',
        onchange: (e) => {
            if (e.target instanceof HTMLSelectElement)
                selectStep(Number(e.target.value), 0);
        },
    }, flows.map((f) => el('option', { value: f.id, selected: f.on }, f.title)));
    return el('div', { class: 'expanded-bar' }, el('div', { class: 'crumbs' }, el('button', { class: 'crumb', onclick: closeAll }, 'Features'), el('span', { class: 'fg-faint' }, '›'), el('button', { class: 'crumb', onclick: collapse }, `#${view.number}`), el('span', { class: 'fg-faint' }, '›'), el('button', { class: 'crumb', onclick: collapse }, 'Flows'), el('span', { class: 'fg-faint' }, '›')), switcher, el('span', { class: 'seq-legend meta', 'aria-label': 'Legend' }, el('span', { class: 'lg call' }), 'call', el('span', { class: 'lg reply' }), 'reply', el('span', { class: 'lg self' }), 'to itself'), el('span', { class: 'grow' }), el('span', { class: 'seg zoom', role: 'group', 'aria-label': 'Zoom' }, el('button', { class: 'segb', 'aria-label': 'Zoom out', onclick: () => zoomBy(-0.1) }, '−'), el('button', {
        class: 'segb zoom-pct',
        title: 'Back to 100%',
        onclick: () => {
            if (!state.expanded)
                return;
            state.expanded.fit = false;
            state.expanded.zoom = 1;
            drawExpanded();
        },
    }, `${Math.round((expanded?.zoom ?? 1) * 100)}%`), el('button', { class: 'segb', 'aria-label': 'Zoom in', onclick: () => zoomBy(0.1) }, '+')), el('button', { class: `btn${expanded?.fit ? ' on' : ''}`, onclick: fit }, 'Fit'), el('button', { class: `btn${expanded?.panel ? ' on' : ''}`, onclick: togglePanel }, expanded?.panel ? 'Hide steps' : 'Show steps'), el('button', { class: 'btn', onclick: collapse }, 'Collapse ', keycap('Esc')));
}
/** Zooms by a tenth at a time, between a quarter and three times. */
function zoomBy(by) {
    if (!state.expanded)
        return;
    state.expanded.fit = false;
    state.expanded.zoom = Math.min(3, Math.max(0.25, Math.round((state.expanded.zoom + by) * 10) / 10));
    drawExpanded();
}
function fit() {
    if (!state.expanded)
        return;
    state.expanded.fit = true;
    drawExpanded();
}
function togglePanel() {
    if (!state.expanded)
        return;
    state.expanded.panel = !state.expanded.panel;
    drawExpanded();
}
function collapse() {
    closeExpanded();
    state.editing = null;
    drawWork();
}
function closeAll() {
    collapse();
    call('open_feature', { slug: null });
}
/** The steps beside the expanded diagram, and the selected step read
 *  whole with what can be done to it. */
function panel(view, flow) {
    const step = flow.step;
    const editing = state.editing;
    let below;
    if (editing?.what === 'step' &&
        step &&
        editing.diagram === flow.id &&
        editing.step === step.number) {
        below = el('div', { class: 'stack-6' }, titled(`Step ${step.number} says`, field('edit-step', editing.text, false, (text) => (editing.text = text), 'what is asked or answered')), formButtons('Save the step'));
    }
    else if (editing?.what === 'flow' && editing.id === flow.id) {
        below = sourceForm(editing);
    }
    else if (step) {
        const lifelines = flow.diagram.lifelines;
        below = el('div', { class: 'stack-12' }, el('div', { class: 'label' }, `Step ${step.number}`, el('span', { class: 'label-aside' }, step.route)), el('div', { class: 'strong' }, step.text), titled('Participants', step.participants.map((label) => {
            const at = lifelines.findIndex((l) => l.label === label);
            const lifeline = lifelines[at];
            const move = (to) => {
                if (lifeline)
                    edit({ on: 'moveParticipant', diagram: flow.id, name: lifeline.name, to });
            };
            return el('div', { class: 'krow' }, el('span', { class: 'grow' }, label), el('button', {
                class: 'icon sm',
                'aria-label': `Move ${label} left`,
                disabled: at <= 0,
                onclick: () => move(at - 1),
            }, '‹'), el('button', {
                class: 'icon sm',
                'aria-label': `Move ${label} right`,
                disabled: at < 0 || at >= lifelines.length - 1,
                onclick: () => move(at + 1),
            }, '›'));
        })), titled('Bound by', step.boundBy.length
            ? lineList(step.boundBy, '✓', 'fg-ok')
            : el('div', { class: 'fg-faint' }, 'No decision binds it yet.')), step.carriedOutBy.length > 0 &&
            titled('Carried out by', lineList(step.carriedOutBy, '·', 'fg-faint')), el('div', { class: 'actions' }, el('button', { class: 'btn', onclick: () => editStep(flow) }, 'Edit step'), el('button', { class: 'btn', onclick: (e) => bindMenu(flow, e) }, 'Bind a decision…'), el('button', { class: 'btn', onclick: () => editSource(flow) }, 'Edit source')));
    }
    return el('aside', { class: 'expanded-panel', 'aria-label': 'Steps' }, el('div', { class: 'label' }, 'Steps', el('span', { class: 'grow' }), el('span', { class: 'label-aside' }, 'drag an arrow to reorder')), el('div', { class: 'stack-2 steps-list' }, flowSteps(flow)), below);
}
export function sourceForm(editing) {
    const title = field('edit-flow-title', editing.title, false, (text) => (editing.title = text), 'What the flow shows');
    const source = field('edit-flow-source', editing.text, true, (text) => (editing.text = text), 'sequenceDiagram');
    delete source.dataset.editing;
    source.classList.add('mono', 'tall');
    return el('div', { class: 'stack-6' }, titled('Title', title), titled('Mermaid source', source), el('div', { class: 'meta' }, 'A comment under a step, ', el('span', { class: 'mono' }, '%% decision #4'), ', binds it.'), formButtons('Save the flow'));
}
export function editStep(flow) {
    const step = flow.step;
    if (step)
        begin({ what: 'step', diagram: flow.id, step: step.number, text: step.text });
}
export function editSource(flow) {
    begin({ what: 'flow', id: flow.id, title: flow.title, text: flow.source });
}
// ---- Dragging -----------------------------------------------------------------
//
// By pointer events, as the board's cards are: one behaviour in every
// webview. A drag only says where it would land; the flow is redrawn in its
// new order when the daemon's pushed flows say so.
const drag = { moved: false, press: null, target: null };
function draggable(drawn, flow, canvas) {
    const local = (e) => {
        const box = drawn.node.getBoundingClientRect();
        const zoom = state.expanded?.zoom ?? 1;
        return { x: (e.clientX - box.left) / zoom, y: (e.clientY - box.top) / zoom };
    };
    const guide = svg('line', {
        class: 'drop-guide',
        x1: 0,
        y1: 0,
        x2: 0,
        y2: 0,
        visibility: 'hidden',
    });
    drawn.node.append(guide);
    drawn.node.addEventListener('pointerdown', (e) => {
        if (e.button !== 0 || !(e.target instanceof Element))
            return;
        const step = stepOf(e.target);
        const head = e.target.closest('[data-lifeline]')?.getAttribute('data-lifeline');
        drag.moved = false;
        drag.target = null;
        if (step !== null)
            drag.press = { kind: 'step', at: step, x: e.clientX, y: e.clientY };
        else if (head != null)
            drag.press = { kind: 'lifeline', at: Number(head), x: e.clientX, y: e.clientY };
        else
            return;
    });
    drawn.node.addEventListener('pointermove', (e) => {
        const press = drag.press;
        if (!press)
            return;
        if (!drag.moved && Math.hypot(e.clientX - press.x, e.clientY - press.y) < 5)
            return;
        // Captured only once it is a drag: a plain click must still land on
        // the arrow it was made on, to pick its step.
        if (!drag.moved)
            drawn.node.setPointerCapture(e.pointerId);
        drag.moved = true;
        canvas.classList.add('dragging');
        const at = local(e);
        if (press.kind === 'step') {
            const nearest = drawn.steps.reduce((best, s) => Math.abs(s.y - at.y) < Math.abs(best.y - at.y) ? s : best);
            drag.target = nearest.step;
            setLine(guide, MARGIN, nearest.y, drawn.width - MARGIN, nearest.y);
        }
        else {
            let nearest = 0;
            drawn.lifelines.forEach((x, i) => {
                if (Math.abs(x - at.x) < Math.abs((drawn.lifelines[nearest] ?? 0) - at.x))
                    nearest = i;
            });
            drag.target = nearest;
            const x = drawn.lifelines[nearest] ?? 0;
            setLine(guide, x, MARGIN, x, drawn.height - MARGIN);
        }
        guide.setAttribute('visibility', 'visible');
    });
    const release = () => {
        const press = drag.press;
        drag.press = null;
        canvas.classList.remove('dragging');
        guide.setAttribute('visibility', 'hidden');
        if (!press || !drag.moved || drag.target === null || drag.target === press.at)
            return;
        if (press.kind === 'step')
            moveStep(flow, press.at, drag.target);
        else {
            const lifeline = flow.diagram.lifelines[press.at];
            if (lifeline)
                edit({ on: 'moveParticipant', diagram: flow.id, name: lifeline.name, to: drag.target });
        }
        // The click that follows the release is the drag's, not a pick.
        setTimeout(() => {
            drag.moved = false;
        }, 0);
    };
    drawn.node.addEventListener('pointerup', release);
    drawn.node.addEventListener('pointercancel', release);
}
/** Sends the step moved, and keeps the cursor on it: the step at its new
 *  place is lit as soon as the pushed flow puts it there. */
function moveStep(flow, step, to) {
    edit({ on: 'moveStep', diagram: flow.id, step, to });
    selectStep(flow.id, to - 1);
}
function setLine(line, x1, y1, x2, y2) {
    line.setAttribute('x1', String(x1));
    line.setAttribute('y1', String(y1));
    line.setAttribute('x2', String(x2));
    line.setAttribute('y2', String(y2));
}
// ---- Keys --------------------------------------------------------------------
/** Runs an expanded diagram's binding, saying whether it had anything to do. */
export function diagramAction(action) {
    const view = state.features?.open;
    const flow = view?.flows?.flow;
    const expanded = state.expanded;
    if (!view || !flow || !expanded)
        return false;
    const number = flow.step?.number ?? 1;
    const count = flow.rows.filter((row) => row.number !== null).length;
    switch (action) {
        case 'next':
        case 'previous':
        case 'first':
        case 'last':
        case 'nextFlow':
        case 'previousFlow':
            call('feature_action', { action });
            return true;
        case 'zoomIn':
        case 'zoomOut':
            zoomBy(action === 'zoomIn' ? 0.1 : -0.1);
            return true;
        case 'zoomReset':
            expanded.fit = false;
            expanded.zoom = 1;
            drawExpanded();
            return true;
        case 'fit':
            fit();
            return true;
        case 'stepsPanel':
            togglePanel();
            return true;
        case 'edit':
            editSource(flow);
            if (!expanded.panel)
                togglePanel();
            return true;
        case 'decide': {
            const lit = document.querySelector('#expanded .smsg.on');
            const box = lit?.getBoundingClientRect();
            bindMenu(flow, box ? { clientX: box.left + 20, clientY: box.bottom + 4 } : null);
            return true;
        }
        case 'earlier':
            if (number > 1)
                moveStep(flow, number, number - 1);
            return true;
        case 'later':
            if (number < count)
                moveStep(flow, number, number + 1);
            return true;
        default:
            return false;
    }
}
