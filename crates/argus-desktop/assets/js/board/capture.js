// A new feature written as one note: the note on the left, and on the right
// what it reads as. The reading is Rust's, by the daemon's own grammar, sent
// back on every change; the page never parses the note itself, so the
// preview is what Create stores. The note stays up after Create until the
// pushed board holds the feature, which then opens.
import { $, el, statusPill } from '../kit/dom.js';
import { actionFor } from '../keys/keymap.js';
import { state } from '../state.js';
import { call } from '../connection/tauri.js';
/** The grammar, as the foot of the note says it. */
const LEGEND = [
    ['first line', '', 'the title'],
    ['a paragraph', '', 'the brief'],
    ['-', 'fg-accent', 'a task; indent it for a subtask'],
    ['?', 'fg-attention', 'an open decision; indented lines are its options'],
    ['!', 'fg-ok', 'a decision already made; the indented line is why'],
];
export function captureOpen() {
    return state.capture?.open === true;
}
export function openCapture() {
    call('capture', { gesture: { on: 'open' } });
}
export function discardCapture() {
    call('capture', { gesture: { on: 'discard' } });
}
function note() {
    const node = document.getElementById('capture-note');
    return node instanceof HTMLTextAreaElement ? node : null;
}
/** The note keeps its lines unwrapped so the gutter's numbers stay beside
 *  them; a line running past the edge fades there instead of being cut. */
function noteCue() {
    const field = note();
    if (!field)
        return;
    const more = field.scrollWidth - field.clientWidth - field.scrollLeft > 4;
    field.classList.toggle('scrolls', more);
    field.title = more ? 'A line runs on: scroll sideways' : '';
}
function create() {
    const text = note()?.value ?? state.capture?.note ?? '';
    call('capture', { gesture: { on: 'create', note: text } });
}
/** Draws the note, keeping the text field when it is already up: the
 *  reading comes back on every key, and redrawing the field would lose the
 *  caret. */
export function drawCapture() {
    const page = state.capture;
    if (!page?.open)
        return;
    const stage = $('fstage');
    const field = note();
    if (field && stage.contains(field)) {
        $('capture-head').replaceWith(head(page));
        $('capture-gutter').replaceWith(gutter(page, field.value));
        $('capture-reads').replaceWith(readsAs(page));
        noteCue();
        return;
    }
    const textarea = el('textarea', {
        class: 'note-field',
        id: 'capture-note',
        'aria-label': 'Feature note',
        spellcheck: 'false',
        placeholder: 'The title on the first line\n\nThe brief under it.\n\n- a task\n  - a subtask\n? an open question\n  - an option',
        oninput: () => {
            call('capture', { gesture: { on: 'note', note: textarea.value } });
            $('capture-gutter').replaceWith(gutter(state.capture ?? page, textarea.value));
            noteCue();
        },
        onscroll: () => {
            const numbers = document.getElementById('capture-gutter');
            if (numbers)
                numbers.scrollTop = textarea.scrollTop;
            noteCue();
        },
        onkeydown: (e) => {
            const action = actionFor(['writing'], e);
            if (action === 'save') {
                e.preventDefault();
                create();
            }
            else if (action === 'closeLayer') {
                e.preventDefault();
                discardCapture();
            }
        },
    });
    textarea.value = page.note;
    stage.replaceChildren(head(page), el('div', { class: 'split capture' }, el('section', { class: 'capture-note stack-8', 'aria-label': 'The note' }, el('div', { class: 'label' }, 'Note'), el('div', { class: 'note-box' }, gutter(page, page.note), textarea), el('div', { class: 'legend' }, LEGEND.map(([mark, tone, means]) => [
        el('span', { class: tone ? `mono ${tone}` : '' }, mark),
        el('span', null, means),
    ]))), readsAs(page)));
    textarea.focus();
    requestAnimationFrame(noteCue);
}
function head(page) {
    return el('div', { class: 'fhead capture-head', id: 'capture-head' }, el('div', { class: 'crumbs' }, el('button', { class: 'crumb', onclick: discardCapture }, 'Features'), el('span', { class: 'fg-faint' }, '›'), el('span', { class: 'fg' }, 'New')), el('div', { class: 'actions head-row' }, el('span', { class: 'type-title' }, `New feature in ${page.repository}`), el('span', { class: 'grow' }), el('button', {
        class: 'btn',
        title: 'Discard the note (Esc)',
        onclick: discardCapture,
    }, 'Discard'), el('button', {
        class: 'btn primary',
        disabled: page.blocked !== null || page.creating,
        title: page.blocked ?? null,
        onclick: create,
    }, page.creating ? 'Creating…' : 'Create', el('span', { class: 'kbd on-accent' }, 'Ctrl Enter'))));
}
/** Line numbers beside the note, a line the grammar could not place
 *  marked where it is. */
function gutter(page, text) {
    const lines = Math.max(1, text.split('\n').length);
    const numbers = el('div', { class: 'note-gutter', id: 'capture-gutter', 'aria-hidden': 'true' }, Array.from({ length: lines }, (_, i) => el('div', { class: page.unread.includes(i + 1) ? 'fg-danger strong' : '' }, String(i + 1))));
    const field = note();
    if (field)
        numbers.scrollTop = field.scrollTop;
    return numbers;
}
function readsAs(page) {
    let verdict;
    if (page.creating)
        verdict = el('span', { class: 'label-aside' }, 'creating…');
    else if (page.blocked)
        verdict = el('span', { class: 'label-aside fg-attention' }, page.blocked);
    else
        verdict = el('span', { class: 'label-aside fg-ok' }, 'ready to create');
    const open = page.decisions.filter((d) => d.open).length;
    const decided = page.decisions.length - open;
    const unread = page.unread.length > 0 && [
        el('div', { class: 'fg-danger' }, page.unread.length === 1
            ? `Line ${page.unread[0]} reads as nothing.`
            : `Lines ${page.unread.join(', ')} read as nothing.`, ' Start a task with -, a question with ?, a decision made with !.'),
    ];
    return el('section', {
        class: 'capture-reads stack-12',
        id: 'capture-reads',
        'aria-label': 'What the note reads as',
    }, el('div', { class: 'label' }, el('span', null, 'Reads as'), el('span', { class: 'grow' }), verdict), el('div', { class: 'cell stack-12' }, unread, el('div', { class: 'stack-4' }, page.title
        ? el('div', { class: 'type-heading' }, page.title)
        : el('div', { class: 'fg-faint' }, 'The first line is the title.'), page.brief && el('div', { class: 'fg-muted prose' }, page.brief)), el('div', { class: 'stack-2' }, el('div', { class: 'label' }, el('span', null, 'Tasks'), el('span', { class: 'grow' }), el('span', { class: 'label-aside' }, `0/${page.tasks.length}`)), page.tasks.length === 0 &&
        el('div', { class: 'meta' }, 'A line starting with - is a task.'), page.tasks.map((task) => el('div', { class: 'nrow', style: `padding-left: ${task.depth * 20}px` }, el('span', { class: 'fg-faint' }, '○'), el('span', { class: 'n' }, task.label), el('span', { class: task.depth ? 'fg-muted' : '' }, task.text)))), page.decisions.length > 0 &&
        el('div', { class: 'stack-2' }, el('div', { class: 'label' }, el('span', null, 'Decisions'), el('span', { class: 'grow' }), open > 0
            ? el('span', { class: 'label-aside fg-attention' }, `${open} open`)
            : el('span', { class: 'label-aside fg-ok' }, `${decided} decided`)), page.decisions.map((decision) => [
            el('div', { class: 'nrow' }, el('span', { class: decision.open ? 'fg-attention' : 'fg-ok' }, decision.open ? '?' : '✓'), el('span', { class: `n ${decision.open ? 'fg-attention' : 'fg-ref'}` }, decision.label), el('span', null, decision.text), decision.open ? statusPill('waiting', 'open') : statusPill('done', 'made')),
            decision.options.length > 0 &&
                el('div', { class: 'opts indent' }, decision.options.map((option) => el('span', { class: 'ktag' }, option))),
            decision.because && el('div', { class: 'meta indent' }, `because ${decision.because}`),
        ]))));
}
