// The history dialog: a checkout's newest commits, each unfolding in
// place to the files it touched, beside the diff of the file under the
// cursor. Rust holds the list and the diffs (`src/review/history.rs`); the
// page draws them and says what was pressed.
import { $, el, ghosts, hint, icon } from '../kit/dom.js';
import { actionFor } from '../keys/keymap.js';
import { diffRow } from './dialog.js';
import { state } from '../state.js';
import { call } from '../connection/tauri.js';
function keycap(key) {
    return el('span', { class: 'kbd' }, key);
}
/**
 * A file is one line: the folder it sits in, dim, before its name, and its
 * counts at the right. A file in the root has no folder to name, so shows
 * only itself.
 */
export function fileRow(r) {
    const cut = r.text.lastIndexOf('/');
    const dir = cut < 0 ? '' : r.text.slice(0, cut + 1);
    return el('button', {
        class: `hrow file${r.on ? ' on' : ''}`,
        title: r.text,
        onclick: () => call('history_action', { action: 'next', row: r.row }),
        ondblclick: () => call('history_action', { action: 'choose', row: null }),
    }, el('span', { class: 'hcfile grow ellipsis' }, dir && el('span', { class: 'hcdir' }, dir), r.text.slice(cut + 1)), el('span', { class: 'fg-ok mono' }, `+${r.added}`), el('span', { class: 'fg-danger mono' }, `−${r.removed}`));
}
/** A commit toggles on a click, which its chevron says; Rust decides what
 *  the click leaves open. */
function commitRow(r) {
    return el('button', {
        class: `hrow commit${r.on ? ' on' : ''}${r.expanded ? ' open' : ''}`,
        'aria-expanded': r.expanded ? 'true' : 'false',
        onclick: () => call('history_action', { action: r.expanded ? 'fold' : 'unfold', row: r.row }),
    }, el('span', { class: 'chev', 'aria-hidden': 'true' }, icon('right')), el('span', { class: 'mono fg-faint' }, r.short), el('span', { class: 'grow ellipsis' }, r.text), r.loading
        ? el('span', { class: 'meta' }, 'loading…')
        : r.expanded &&
            r.files !== null &&
            el('span', { class: 'meta' }, `${r.files} file${r.files === 1 ? '' : 's'}`), el('span', { class: 'meta' }, r.ago));
}
function listRow(r) {
    return r.kind === 'file' ? fileRow(r) : commitRow(r);
}
/** The commits' small header: what the list is, and the way to fold it all
 *  by mouse. */
function listHead() {
    return el('div', { class: 'history-head' }, el('span', { class: 'grow' }, 'Commits'), el('button', {
        class: 'btn sm quiet',
        title: 'Fold every commit',
        onclick: () => call('history_action', { action: 'foldAll', row: null }),
    }, icon('fold-all'), 'Fold all'));
}
function diff(h) {
    let body;
    if (!h.path)
        body = el('div', { class: 'review-empty fg-muted' }, 'Open a commit and pick a file.');
    else if (h.diffEmpty)
        body = el('div', { class: 'review-empty fg-muted' }, h.diffEmpty);
    else if (h.diffLoading)
        body = ghosts([[0.4], [0.9], [0.8], [0.85], [0.6]]);
    else
        body = h.diff.map((d) => diffRow(d));
    return el('section', { class: 'review-diff' }, h.path &&
        el('div', { class: 'fhdr' }, el('span', { class: 'mono fg-faint', title: 'Diffed against its first parent' }, h.commit ?? ''), 
        // The name leads and the folder after it is what gives way, as in
        // the file list: a path cut at its end loses the name.
        el('span', { class: 'mono cut', title: h.path }, el('span', { class: 'strong' }, h.path.slice(h.path.lastIndexOf('/') + 1)), h.path.includes('/') &&
            el('span', { class: 'fg-faint' }, `  ${h.path.slice(0, h.path.lastIndexOf('/'))}`)), el('span', { class: 'grow' }), el('button', {
            class: 'btn sm',
            onclick: () => call('history_action', { action: 'asReview', row: null }),
        }, 'Open as a review ', keycap('O'))), el('div', { class: 'dbody', id: 'history-body' }, body));
}
/**
 * Rust filters the rows; the field only says what it says. Its text is its
 * own once drawn: an echo that lands after the next letter would drop it.
 */
function filterField(filter) {
    const live = document.getElementById('history-filter');
    const field = el('input', {
        class: 'field filter',
        id: 'history-filter',
        placeholder: 'Filter commits',
        'aria-label': 'Filter commits by subject, hash or author',
        oninput: (e) => {
            if (e.target instanceof HTMLInputElement)
                call('filter_history', { filter: e.target.value });
        },
        onkeydown: (e) => {
            // The list has the keys back, filtered.
            if (actionFor(['filter'], e) === 'leave') {
                e.preventDefault();
                e.target.blur();
            }
        },
    });
    field.value = live instanceof HTMLInputElement ? live.value : filter;
    return field;
}
/** `/`: the filter field takes the keys. */
export function focusHistoryFilter() {
    document.getElementById('history-filter')?.focus();
}
export function drawHistory() {
    const h = state.history;
    const host = $('history');
    if (!h?.open) {
        host.hidden = true;
        host.replaceChildren();
        return;
    }
    const filterHad = document.activeElement?.id === 'history-filter';
    const scrolled = ['history-list', 'history-body'].map((id) => document.getElementById(id)?.scrollTop ?? 0);
    host.hidden = false;
    let list;
    if (h.loading)
        list = ghosts([[0.8], [0.7], [0.9], [0.6], [0.75], [0.85]]);
    else if (h.empty)
        list = el('div', { class: 'review-empty fg-muted' }, h.empty);
    else
        list = h.rows.map(listRow);
    host.replaceChildren(el('div', { class: 'scrim' }, el('section', {
        class: 'dialog sheet-dialog review-dialog',
        role: 'dialog',
        'aria-modal': 'true',
        'aria-labelledby': 'history-title',
    }, el('div', { class: 'dhead review-head' }, el('div', { class: 'type-title', id: 'history-title' }, 'History'), el('span', { class: 'chip-branch', title: h.name }, icon('branch'), el('span', { class: 'mono' }, h.branch)), el('span', { class: 'meta' }, 'newest 100 on HEAD'), el('span', { class: 'grow' }), filterField(h.filter), el('button', {
        class: 'icon',
        'aria-label': 'Close the history',
        onclick: () => call('history_action', { action: 'closeLayer', row: null }),
    }, icon('close'))), el('div', { class: 'review-split' }, el('div', { class: 'history-col' }, listHead(), el('aside', { class: 'history-list', id: 'history-list', 'aria-label': 'Commits' }, list)), diff(h)), el('div', { class: 'dfoot meta' }, hint(['J', 'K'], 'move'), hint(['L', 'H'], 'open / fold'), hint('Shift H', 'fold all'), hint('Enter', 'as a review'), hint('/', 'filter'), hint('Esc', 'close')))));
    ['history-list', 'history-body'].forEach((id, i) => {
        const node = document.getElementById(id);
        if (node)
            node.scrollTop = scrolled[i] ?? 0;
    });
    host.querySelector('.hrow.on')?.scrollIntoView({ block: 'nearest' });
    // Redrawn as each letter arrives: the filter keeps the keys and the caret.
    const field = document.getElementById('history-filter');
    if (filterHad && field instanceof HTMLInputElement) {
        field.focus();
        field.setSelectionRange(field.value.length, field.value.length);
    }
}
export function historyOpen() {
    return state.history?.open === true;
}
