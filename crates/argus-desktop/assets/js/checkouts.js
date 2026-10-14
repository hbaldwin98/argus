// The Checkouts stage: the chosen repository's checkouts as an operational
// table, the branches without one beneath them while shown, and the
// selected checkout said in full along the foot. Rust builds every row
// (`src/checkouts.rs`); the page draws them and reports what is chosen.
import { startMenu, toggleStart } from './chrome.js';
import { $, el, icon } from './dom.js';
import { state } from './state.js';
import { call } from './tauri.js';
function keycap(key) {
    return el('span', { class: 'kbd' }, key);
}
function empty(...body) {
    return el('div', { class: 'empty' }, el('div', { class: 'empty-body' }, body));
}
function head(c) {
    const filter = el('input', {
        class: 'field filter',
        id: 'checkouts-filter',
        placeholder: 'Filter branches',
        'aria-label': 'Filter checkouts and branches',
        oninput: (e) => {
            if (e.target instanceof HTMLInputElement)
                call('filter_checkouts', { filter: e.target.value });
        },
        onkeydown: (e) => {
            if (e.key === 'Escape' || e.key === 'Enter') {
                e.preventDefault();
                e.target.blur();
            }
        },
    });
    filter.value = c.filter;
    const branches = `${c.branches} branch${c.branches === 1 ? '' : 'es'} without one`;
    return el('div', { class: 'fhead index-head' }, el('div', { class: 'crumbs' }, el('span', { class: 'fg' }, 'Checkouts'), el('span', { class: 'grow' }), el('span', { class: 'meta' }, keycap('Enter'), ' open · ', keycap('B'), ' branches · ', keycap('F'), ' fetch · ', keycap('P'), ' pull')), el('div', { class: 'actions index-tools' }, el('span', { class: 'type-title' }, `Checkouts in ${c.repository}`), el('span', { class: 'meta' }, `${c.checkouts} checkout${c.checkouts === 1 ? '' : 's'} · ${branches}`, c.main && [' · main is ', el('span', { class: 'mono fg-ref' }, c.main)]), el('span', { class: 'grow' }), filter, el('button', {
        class: 'btn',
        'aria-pressed': String(c.branchesShown),
        onclick: () => call('checkouts_action', { action: 'branches' }),
    }, 'Branches ', keycap('B')), el('button', { class: 'btn', onclick: () => call('checkouts_action', { action: 'fetch' }) }, 'Fetch all ', keycap('F'))));
}
function running(row) {
    if (!row.running.length)
        return el('span', { class: 'fg-faint' }, '—');
    return row.running.flatMap((pane, i) => [
        i > 0 && el('span', { class: 'fg-faint' }, ' · '),
        el('span', { class: `glyph${pane.agent ? '' : ' sh'}` }, pane.agent ? '◆' : '›'),
        el('span', { class: `st ${pane.status}` }, pane.status),
    ]);
}
function actions(row) {
    const open = el('button', { class: 'btn sm', onclick: () => call('checkouts_action', { action: 'choose' }) }, 'Open');
    if (row.checkout === null)
        return open;
    const checkout = row.checkout;
    const soon = (label, key) => el('button', { class: 'btn sm', disabled: true, title: 'Not in this window yet' }, `${label} `, keycap(key));
    const hot = state.start?.checkout === checkout && state.start.from === 'table';
    return [
        open,
        soon('Review', 'R'),
        soon('History', 'H'),
        el('button', { class: 'btn sm', onclick: () => call('checkouts_action', { action: 'pull' }) }, 'Pull ', keycap('P')),
        el('span', { class: 'start-at' }, el('button', {
            class: `icon${hot ? ' hot' : ''}`,
            title: 'Start a shell or an agent',
            'aria-label': `Start a shell or an agent on ${row.name ?? row.branch}`,
            'aria-expanded': String(hot),
            onclick: (e) => {
                e.stopPropagation();
                toggleStart(checkout, 'table');
            },
        }, icon('plus')), hot && startMenu(row.name ?? row.branch, checkout)),
    ];
}
function tableRow(row, selected) {
    return el('tr', {
        class: selected ? 'on' : null,
        'aria-selected': String(selected),
        onclick: () => call('select_checkout_row', { key: row.key }),
        ondblclick: () => call('checkouts_action', { action: 'choose' }),
    }, el('td', null, row.name === null
        ? el('span', { class: 'fg-faint' }, '—')
        : [
            el('b', null, row.name),
            ' ',
            el('span', { class: 'meta' }, row.primary ? 'primary' : 'worktree'),
        ]), el('td', { class: 'mono fg-ref' }, row.branch), el('td', null, row.git !== null && el('span', { class: `git${row.clean ? ' clean' : ''}` }, row.git), row.note && el('span', { class: 'meta' }, row.note)), el('td', null, running(row)), el('td', { class: `mono${row.viewed ? ' fg-accent' : ''}` }, row.feature === null ? el('span', { class: 'fg-faint' }, '—') : `#${row.feature}`), el('td', { class: 'row-actions' }, selected && actions(row)));
}
function group(label) {
    return el('tr', { class: 'group' }, el('td', { colspan: '6' }, label));
}
function table(c) {
    const checkouts = c.rows.filter((row) => row.checkout !== null);
    const branches = c.rows.filter((row) => row.checkout === null);
    const rows = (list) => list.map((row) => tableRow(row, row.key === c.selected));
    return el('div', { class: 'ctable-wrap' }, el('table', { class: 'ctable', 'aria-label': `Checkouts in ${c.repository}` }, el('thead', null, el('tr', null, ['Checkout', 'Branch', 'Git', 'Running', 'Feature'].map((name) => el('th', null, name)), el('th', null, el('span', { class: 'sr-only' }, 'Actions')))), el('tbody', null, checkouts.length > 0 && [group('Checkouts'), rows(checkouts)], branches.length > 0 && [group('Branches without a checkout'), rows(branches)])));
}
function stack(label, value) {
    return el('div', { class: 'stack-2' }, el('span', { class: 'mlabel' }, label), el('span', { class: 'meta' }, value));
}
function detail(c) {
    const d = c.detail;
    return el('div', { class: 'cdetail' }, d && [
        stack(d.name, el('span', { class: 'mono' }, d.path)),
        d.upstream && stack('Upstream', d.upstream),
        d.changes && stack('Changes', d.changes),
    ], el('span', { class: 'grow' }), el('span', { class: 'meta cnote' }, 'A branch row opens by switching the primary checkout to it. Fetch and pull run in the selected checkout, or the primary one for a branch.'));
}
export function drawCheckouts() {
    if (state.rail.stage !== 'checkouts')
        return;
    const stage = $('cstage');
    const c = state.checkouts;
    if (!c || !c.repository) {
        stage.replaceChildren(empty(icon('folder'), el('div', { class: 'type-title' }, 'No repository'), el('div', { class: 'fg-muted' }, 'Choose a repository on the left to list its checkouts.')));
        return;
    }
    const filterHad = document.activeElement?.id === 'checkouts-filter';
    const body = c.rows.length
        ? table(c)
        : empty(el('div', { class: 'fg-muted' }, c.filter ? `Nothing matches “${c.filter}”.` : 'No checkouts.'));
    stage.replaceChildren(head(c), body, detail(c));
    stage.querySelector('tr.on')?.scrollIntoView({ block: 'nearest' });
    // Redrawn as each letter arrives: the filter keeps the keys and the caret.
    if (filterHad) {
        const again = document.getElementById('checkouts-filter');
        if (again instanceof HTMLInputElement) {
            again.focus();
            again.setSelectionRange(again.value.length, again.value.length);
        }
    }
}
/** Runs a Checkouts key, saying whether it took it. */
export function checkoutsAction(action) {
    if (action === 'filter') {
        $('checkouts-filter').focus();
        return true;
    }
    call('checkouts_action', { action });
    return true;
}
