// The recent-sessions dialog: a repository's agent sessions, live ones
// first, beside what the one under the cursor said, with the way to go to
// it, resume it, or read it whole in the reader `transcript` draws in
// place of the list. Rust holds the list, the filters and what each
// session offers (`src/agents/sessions_dialog.rs`); the page draws them
// and says what was pressed.
import { $, el, ghosts, hint, icon } from '../kit/dom.js';
import { actionFor } from '../keys/keymap.js';
import { openMenu } from '../kit/menu.js';
import { state } from '../state.js';
import { call } from '../connection/tauri.js';
import { drawReader, placeReader, said } from './transcript.js';
function keycap(key) {
    return el('span', { class: 'kbd' }, key);
}
function act(action) {
    call('sessions_action', { action, row: null });
}
/** "claude on main · 31 turns": what ran where, as a sentence. */
function sessionMeta(r) {
    const turns = r.turns === null ? null : `${r.turns} turn${r.turns === 1 ? '' : 's'}`;
    const where = r.removed ? `${r.checkout}, since removed` : r.checkout;
    return [`${r.harness} on ${where}`, turns].filter(Boolean).join(' \u00b7 ');
}
/** The group says which are live, so a row does not: an ended one keeps its
 *  age, the only thing that tells it from its neighbours. */
function listRow(r) {
    return el('button', {
        class: `hrow srow${r.on ? ' on' : ''}${r.removed ? ' removed' : ''}`,
        title: r.removed ? `ran in ${r.checkout}, a worktree since removed` : null,
        onclick: () => call('sessions_action', { action: 'next', row: r.row }),
        ondblclick: () => act('choose'),
    }, el('span', { class: 'scol grow' }, el('span', { class: 'stitle ellipsis' }, r.title), el('span', { class: 'smeta meta ellipsis' }, sessionMeta(r))), !r.live && el('span', { class: 'meta' }, r.ago));
}
/** "Ended \u00b7 4 \u00b7 newest 20m ago". */
function endedSummary(s) {
    const newest = s.endedNewest && s.endedNewest !== '\u2014' ? ` \u00b7 newest ${s.endedNewest} ago` : '';
    return `${s.endedCount}${newest}`;
}
/** The rows under a heading for the live ones and one for the rest, which
 *  folds to a single row. */
function list(s) {
    if (s.loading)
        return ghosts([[0.8], [0.7], [0.9], [0.6], [0.75], [0.85]]);
    if (s.empty)
        return el('div', { class: 'review-empty fg-muted' }, s.empty);
    const live = s.rows.filter((r) => r.live);
    const ended = s.rows.filter((r) => !r.live);
    // A search or a filter asks for what is in the group, and a listing with
    // nothing live has no row to rest the cursor on but these: neither folds.
    const folds = live.length > 0 && !s.search.trim() && s.harness === null && s.checkout === null;
    return [
        live.length > 0 && el('div', { class: 'mlabel sgroup' }, 'Live'),
        live.map(listRow),
        s.endedCount > 0 &&
            (folds
                ? el('button', {
                    class: 'sgroup sfold',
                    'aria-expanded': s.endedFolded ? 'false' : 'true',
                    onclick: () => act('toggleEnded'),
                }, el('span', { class: 'chev', 'aria-hidden': 'true' }, icon('right')), 'Ended', el('span', { class: 'meta' }, endedSummary(s)))
                : el('div', { class: 'mlabel sgroup' }, 'Ended')),
        ended.map(listRow),
    ];
}
function transcript(p) {
    if (p.loading)
        return ghosts([[0.5], [0.9], [0.8], [0.4], [0.85], [0.6]]);
    if (p.notice)
        return el('div', { class: 'review-empty fg-muted' }, p.notice);
    return [
        p.earlier > 0 &&
            el('div', { class: 'meta tearlier' }, `${p.earlier} earlier entr${p.earlier === 1 ? 'y' : 'ies'} not shown`),
        p.entries.map((l) => said(l)),
    ];
}
/** The daemon sends the moment a session began as an ISO instant; a person
 *  reads it as a day and a time in their own zone. */
function humanTime(iso) {
    const at = new Date(iso);
    if (Number.isNaN(at.getTime()))
        return iso;
    return at.toLocaleString(undefined, {
        month: 'short',
        day: 'numeric',
        hour: 'numeric',
        minute: '2-digit',
    });
}
function preview(s) {
    const p = s.preview;
    return el('section', { class: 'review-diff' }, p &&
        el('div', { class: 'fhdr' }, el('span', { class: 'ellipsis' }, p.title), el('span', { class: 'meta' }, p.harness), el('span', { class: 'meta mono scheckout' }, p.checkout), p.started &&
            el('span', { class: 'meta', title: p.started }, `started ${humanTime(p.started)}`), el('span', { class: 'grow' }), 
        // The whole id is for finding the file; eight characters tell
        // sessions apart, and the rest is a hover away.
        el('span', { class: 'meta mono sid', title: p.sessionId }, p.sessionId.slice(0, 8))), el('div', { class: 'dbody transcript', id: 'sessions-body' }, p ? transcript(p) : el('div', { class: 'review-empty fg-muted' }, 'Pick a session.')));
}
/** The reader, for a session whose harness keeps a transcript to open. */
function readButton(p) {
    return (p.readable &&
        el('button', { class: 'btn sm', onclick: () => act('transcript') }, 'Open full transcript ', keycap('O')));
}
/** What the session under the cursor offers, named in full. */
function offer(p) {
    if (!p)
        return null;
    if (p.offer === 'go') {
        return [
            readButton(p),
            el('button', { class: 'btn sm primary', onclick: () => act('choose') }, 'Go to its pane ', el('span', { class: 'kbd on-accent' }, 'Enter')),
        ];
    }
    if (p.offer === 'resume') {
        return [
            el('span', { class: 'meta' }, 'runs'),
            el('code', { class: 'mono scommand ellipsis', title: p.command }, p.command),
            readButton(p),
            el('button', { class: 'btn sm primary', onclick: () => act('resume') }, 'Resume in a new pane ', el('span', { class: 'kbd on-accent' }, 'R')),
        ];
    }
    return [
        p.why && el('span', { class: 'meta ellipsis' }, `Not resumable: ${p.why}`),
        readButton(p),
    ];
}
/**
 * Rust filters the rows; the field only says what it says. Its text is its
 * own once drawn: an echo that lands after the next letter would drop it.
 */
function searchField(search) {
    const live = document.getElementById('sessions-search');
    const field = el('input', {
        class: 'field filter',
        id: 'sessions-search',
        placeholder: 'Search sessions',
        'aria-label': 'Search sessions by title, harness, checkout or id',
        oninput: (e) => {
            if (e.target instanceof HTMLInputElement)
                call('search_sessions', { search: e.target.value });
        },
        onkeydown: (e) => {
            // The list has the keys back, filtered.
            if (actionFor(['filter'], e) === 'leave') {
                e.preventDefault();
                e.target.blur();
            }
        },
    });
    field.value = live instanceof HTMLInputElement ? live.value : search;
    return field;
}
/** A choice of the popover: all of them, or one the listing holds. */
function choices(heading, all, options, chosen, pick) {
    const item = (label, value) => ({
        label,
        run: () => pick(value),
        hint: chosen === value ? icon('check') : false,
    });
    return [{ heading }, item(all, null), ...options.map((o) => item(o, o))];
}
/** One button for both filters: they are seldom set, and two selects in the
 *  head spent its room on what is usually "Every ...". */
function filterButton(s) {
    const active = [s.harness, s.checkout].filter((v) => v !== null);
    const button = el('button', {
        class: 'btn sm',
        'aria-haspopup': 'menu',
        onclick: () => {
            const at = button.getBoundingClientRect();
            openMenu('Filter sessions', [
                ...choices('Harness', 'Every harness', s.harnesses, s.harness, (h) => filters(h, s.checkout)),
                'sep',
                ...choices('Checkout', 'Every checkout', s.checkouts, s.checkout, (c) => filters(s.harness, c)),
            ], { x: at.right, y: at.bottom + 4, align: 'end' });
        },
    }, icon('filter'), 'Filter', active.length > 0 && el('span', { class: 'meta' }, active.join(' \u00b7 ')));
    return button;
}
function filters(harness, checkout) {
    call('filter_sessions', { harness, checkout });
}
/** The session whose transcript the body last showed, read. */
let drawnRead = null;
/** `/`: the search field takes the keys. */
export function focusSessionsSearch() {
    document.getElementById('sessions-search')?.focus();
}
export function drawSessions() {
    const s = state.sessions;
    const host = $('sessions');
    if (!s?.open) {
        host.hidden = true;
        host.replaceChildren();
        return;
    }
    const searchHad = document.activeElement?.id === 'sessions-search';
    const findHad = document.activeElement?.id === 'transcript-find';
    const scrolled = ['sessions-list', 'sessions-body', 'reader-body'].map((id) => document.getElementById(id)?.scrollTop ?? 0);
    host.hidden = false;
    if (s.reader) {
        host.replaceChildren(el('div', { class: 'scrim' }, drawReader(s.reader)));
        placeReader(s.reader, scrolled[2] ?? 0);
        drawnRead = null;
        // Redrawn as each letter arrives: the find keeps the keys and the caret.
        const field = document.getElementById('transcript-find');
        if (findHad && field instanceof HTMLInputElement) {
            field.focus();
            field.setSelectionRange(field.value.length, field.value.length);
        }
        return;
    }
    placeReader(null, 0);
    host.replaceChildren(el('div', { class: 'scrim' }, el('section', {
        class: 'dialog sheet-dialog review-dialog',
        role: 'dialog',
        'aria-modal': 'true',
        'aria-labelledby': 'sessions-title',
    }, el('div', { class: 'dhead review-head' }, el('div', { class: 'type-title', id: 'sessions-title' }, 'Recent sessions · ', s.repository), el('span', { class: 'meta' }, 'newest 100, every checkout'), el('span', { class: 'grow' }), searchField(s.search), filterButton(s), el('button', {
        class: 'icon',
        'aria-label': 'Close the recent sessions',
        onclick: () => act('closeLayer'),
    }, icon('close'))), el('div', { class: 'review-split' }, el('aside', { class: 'history-list', id: 'sessions-list', 'aria-label': 'Sessions' }, list(s)), preview(s)), el('div', { class: 'dfoot meta' }, hint(['J', 'K'], 'move'), hint('/', 'search'), hint(['H', 'C'], 'harness, checkout'), hint('Esc', 'close'), el('span', { class: 'grow' }), offer(s.preview)))));
    ['sessions-list', 'sessions-body'].forEach((id, i) => {
        const node = document.getElementById(id);
        if (node)
            node.scrollTop = scrolled[i] ?? 0;
    });
    // A transcript newly read opens on how it ended, which is what a
    // preview is opened for; one redrawn stays where it was scrolled.
    const read = s.preview && !s.preview.loading ? s.preview.sessionId : null;
    const body = document.getElementById('sessions-body');
    if (body && read !== drawnRead)
        body.scrollTop = body.scrollHeight;
    drawnRead = read;
    host.querySelector('.hrow.on')?.scrollIntoView({ block: 'nearest' });
    // Redrawn as each letter arrives: the search keeps the keys and the caret.
    const field = document.getElementById('sessions-search');
    if (searchHad && field instanceof HTMLInputElement) {
        field.focus();
        field.setSelectionRange(field.value.length, field.value.length);
    }
}
export function sessionsOpen() {
    return state.sessions?.open === true;
}
/** The full transcript is open in place of the list, and holds its keys. */
export function readerOpen() {
    return sessionsOpen() && state.sessions?.reader != null;
}
