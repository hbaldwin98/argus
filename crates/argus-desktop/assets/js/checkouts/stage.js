// The Checkouts stage: the chosen repository's checkouts as an operational
// table, the branches without one beneath them while shown, and the
// selected checkout said in full along the foot. Rust builds every row
// (`src/checkouts/table.rs`); the page draws them and reports what is
// chosen.
import { startMenu, toggleStart } from '../chrome.js';
import { $, el, ghosts, icon, statusPill } from '../kit/dom.js';
import { closeLayer, openMenu } from '../kit/menu.js';
import { actionFor } from '../keys/keymap.js';
import { state } from '../state.js';
import { call } from '../connection/tauri.js';
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
            if (actionFor(['filter'], e) === 'leave') {
                e.preventDefault();
                e.target.blur();
            }
        },
    });
    filter.value = c.filter;
    const branches = `${c.branches} other branch${c.branches === 1 ? '' : 'es'}`;
    return el('div', { class: 'fhead index-head' }, el('div', { class: 'actions index-tools' }, el('span', { class: 'meta' }, `${c.checkouts} checkout${c.checkouts === 1 ? '' : 's'} · ${branches}`, c.main && [' · on ', el('span', { class: 'mono fg-ref' }, c.main)]), el('span', { class: 'grow' }), filter, el('button', {
        class: 'btn',
        'aria-pressed': String(c.branchesShown),
        onclick: () => call('checkouts_action', { action: 'branches' }),
    }, 'Branches ', keycap('B')), el('button', { class: 'btn', onclick: () => call('checkouts_action', { action: 'fetch' }) }, 'Fetch all ', keycap('F')), el('button', { class: 'btn primary', onclick: () => checkoutsAction('newWorktree') }, 'New worktree')));
}
/** The keys the table answers to, quietly under it. */
function keyBar() {
    const keys = [
        ['Enter', 'open'],
        ['R', 'review'],
        ['H', 'history'],
        ['P', 'pull'],
        ['N', 'new worktree'],
        ['D', 'delete'],
    ];
    return el('div', { class: 'kbar' }, keys.map(([key, what]) => el('span', { class: 'kbar-key' }, keycap(key), ` ${what}`)));
}
function running(row) {
    if (!row.running.length)
        return null;
    return [
        row.shared !== null &&
            el('span', { class: 'shared' }, icon('people'), ` shared by ${row.shared} `),
        row.running.map((pane) => el('span', { class: 'pane-status', title: pane.agent ? 'an agent' : 'a shell' }, statusPill(pane.status))),
    ];
}
/** Puts the cursor on a row before an action that works on the cursor's
 *  row; a click on the row's own button must not wait for a second one. */
function onRow(row, run) {
    return (e) => {
        e.stopPropagation();
        if (state.checkouts?.selected !== row.key)
            call('select_checkout_row', { key: row.key });
        run();
    };
}
/** What is done to a row, in the trailing column, on every row: the one
 *  thing most wanted, the start menu, and the rest behind the overflow.
 *  Quiet until the row is hovered or selected, and never over a cell. */
function actions(row) {
    const checkout = row.checkout;
    const name = row.name ?? row.branch;
    const hot = checkout !== null && state.start?.checkout === checkout && state.start.from === 'table';
    const primary = checkout === null
        ? el('button', {
            class: 'btn sm',
            onclick: onRow(row, () => checkoutsAction('newWorktree')),
        }, 'Check out')
        : el('button', {
            class: 'btn sm',
            onclick: onRow(row, () => call('checkouts_action', { action: 'choose' })),
        }, 'Open');
    return el('div', { class: 'ra' }, primary, checkout !== null &&
        el('span', { class: 'start-at' }, el('button', {
            class: `icon${hot ? ' hot' : ''}`,
            title: 'Start a shell or an agent',
            'aria-label': `Start a shell or an agent on ${name}`,
            'aria-expanded': String(hot),
            onclick: (e) => {
                e.stopPropagation();
                toggleStart(checkout, 'table');
            },
        }, icon('plus')), hot && startMenu(name, checkout)), el('button', {
        class: 'icon',
        title: 'More actions',
        'aria-label': `More actions for ${name}`,
        onclick: (e) => {
            e.stopPropagation();
            const at = e.currentTarget.getBoundingClientRect();
            const choose = (run) => () => {
                if (state.checkouts?.selected !== row.key)
                    call('select_checkout_row', { key: row.key });
                run();
            };
            openMenu(`More actions for ${name}`, [
                checkout !== null && {
                    label: 'Review',
                    run: () => call('open_review', { checkout }),
                },
                checkout !== null && {
                    label: 'History',
                    run: () => call('open_history', { checkout }),
                },
                checkout !== null && {
                    label: 'Pull',
                    run: choose(() => call('checkouts_action', { action: 'pull' })),
                },
                checkout === null && {
                    label: 'New worktree',
                    run: choose(() => checkoutsAction('newWorktree')),
                },
                !row.primary && 'sep',
                !row.primary && {
                    label: checkout === null ? 'Delete' : 'Remove',
                    danger: true,
                    run: choose(() => call('checkouts_action', { action: 'delete' })),
                },
            ], { x: at.right - 180, y: at.bottom + 4 });
        },
    }, '⋯'));
}
function tableRow(row, selected) {
    return el('tr', {
        class: selected ? 'on' : null,
        'aria-selected': String(selected),
        onclick: () => call('select_checkout_row', { key: row.key }),
        ondblclick: () => call('checkouts_action', { action: 'choose' }),
    }, el('td', null, row.name !== null
        ? [
            el('span', { class: 'cname' }, row.name),
            el('span', { class: 'ctag' }, row.primary ? 'primary' : 'worktree'),
        ]
        : // A branch with no directory is named where a checkout is, so the
            // first column is never blank.
            el('span', { class: 'mono fg-ref bname' }, icon('branch'), row.branch)), el('td', { class: 'mono fg-ref' }, row.name !== null && row.branch), el('td', null, row.git !== null && el('span', { class: `git${row.clean ? ' clean' : ''}` }, row.git), row.note && el('span', { class: 'meta' }, row.note)), el('td', { class: 'col-running' }, running(row)), el('td', { class: `col-feature mono${row.viewed ? ' fg-accent' : ''}` }, row.feature !== null && `#${row.feature}`), el('td', { class: 'col-last' }, row.last && commitLine(row.last)), el('td', { class: 'row-actions' }, actions(row)));
}
function commitLine(commit) {
    return [
        el('span', { class: 'mono fg-faint' }, commit.short),
        ' ',
        el('span', { class: 'fg-muted' }, commit.summary),
        el('span', { class: 'fg-faint' }, ` · ${commit.ago}`),
    ];
}
function act(label, key, run, danger = false) {
    return el('button', { class: `btn sm${danger ? ' danger-line' : ''}`, onclick: run }, label, ' ', keycap(key));
}
/** The selected row's own strip, directly under it: where it is, what it
 *  tracks, what changed, and what can be done to it. */
function detailRow(c, row) {
    const d = c.detail;
    if (!d)
        return null;
    const checkout = row.checkout;
    return el('tr', { class: 'detail-row' }, el('td', { colspan: '7' }, el('div', { class: 'cdetail' }, el('div', { class: 'cfacts' }, stack('Path', el('span', { class: 'mono', title: d.path }, repoRelative(d.path))), d.upstream && stack('Upstream', d.upstream), d.changes && stack('Changes', d.changes)), el('span', { class: 'grow' }), el('div', { class: 'cacts' }, checkout !== null && [
        act('Review', 'R', () => call('open_review', { checkout })),
        act('History', 'H', () => call('open_history', { checkout })),
        act('Pull', 'P', () => call('checkouts_action', { action: 'pull' })),
    ], checkout === null && act('New worktree', 'N', () => checkoutsAction('newWorktree')), !row.primary &&
        act(checkout === null ? 'Delete' : 'Remove', 'D', () => call('checkouts_action', { action: 'delete' }), true)))));
}
function group(label) {
    return el('tr', { class: 'group' }, el('td', { colspan: '7' }, label));
}
function table(c) {
    const checkouts = c.rows.filter((row) => row.checkout !== null);
    const branches = c.rows.filter((row) => row.checkout === null);
    const rows = (list) => list.flatMap((row) => {
        const selected = row.key === c.selected;
        return [tableRow(row, selected), selected ? detailRow(c, row) : null];
    });
    return el('div', { class: 'ctable-wrap' }, el('table', { class: 'ctable', 'aria-label': `Checkouts in ${c.repository}` }, el('thead', null, el('tr', null, ['Checkout', 'Branch', 'Git', 'Running', 'Feature', 'Last commit'].map((name) => el('th', { class: `col-${name.split(' ')[0]?.toLowerCase()}` }, name)), el('th', { class: 'col-actions' }, 'Actions'))), el('tbody', null, checkouts.length > 0 && [group('Checkouts'), rows(checkouts)], branches.length > 0 && [group('Branches without a checkout'), rows(branches)])));
}
/** A worktree's path from the repository's own `.argus/` folder when it
 *  lives under one, whole otherwise; the absolute path stays in the tooltip. */
function repoRelative(path) {
    const flat = path.replaceAll('\\', '/');
    const at = flat.indexOf('/.argus/');
    return at >= 0 ? flat.slice(at + 1) : path;
}
function stack(label, value) {
    return el('div', { class: 'stack-2' }, el('span', { class: 'mlabel' }, label), el('span', { class: 'meta' }, value));
}
// ---- New worktree and the questions before deleting ----------------------
/** A new branch off the selected checkout, in a worktree of its own. */
function nameDialog(from) {
    const name = el('input', {
        class: 'field mono',
        id: 'worktree-name',
        placeholder: 'desktop-checkouts',
    });
    const make = () => {
        call('new_worktree', { name: name.value });
        closeLayer();
    };
    name.addEventListener('keydown', (e) => {
        const action = actionFor(['writing'], e);
        if (action === 'choose')
            make();
        if (action === 'closeLayer')
            closeLayer();
    });
    $('layer').replaceChildren(el('div', {
        class: 'scrim',
        onclick: (e) => {
            if (e.target === e.currentTarget)
                closeLayer();
        },
    }, el('section', {
        class: 'dialog hold-dialog',
        role: 'dialog',
        'aria-modal': 'true',
        'aria-labelledby': 'worktree-title',
    }, el('div', { class: 'dhead' }, el('div', { class: 'type-title', id: 'worktree-title' }, 'New worktree')), el('div', { class: 'dbody' }, el('label', { class: 'label', for: 'worktree-name' }, 'Branch'), name, el('div', { class: 'meta' }, 'Off ', el('span', { class: 'mono' }, from), '.')), el('div', { class: 'dfoot' }, el('span', { class: 'grow' }), el('button', { class: 'btn', onclick: closeLayer }, 'Cancel'), el('button', { class: 'btn primary', onclick: make }, 'Create')))));
    name.focus();
}
/** Says no to the stage's question, as Escape or a click outside does. */
export function answerNo() {
    if (!state.checkouts?.confirm)
        return false;
    call('answer_checkouts', { yes: false });
    closeLayer();
    return true;
}
function confirmDialog(question) {
    const force = question.kind === 'force';
    const title = force
        ? ['Delete ', el('span', { class: 'mono' }, question.branch), ' anyway?']
        : ['Remove ', el('span', { class: 'mono' }, question.name), '?'];
    const body = force
        ? [
            el('div', { class: 'fg-muted' }, 'It has commits no other branch has.'),
            question.commits.length > 0 &&
                el('div', { class: 'commits' }, question.commits.map((commit) => el('div', null, commitLine(commit)))),
            el('div', { class: 'meta' }, 'Local only: nothing is pushed.'),
        ]
        : [el('div', { class: 'fg-muted' }, 'Its panes end; its directory and branch are deleted.')];
    return el('div', {
        class: 'scrim',
        id: 'checkouts-confirm',
        onclick: (e) => {
            if (e.target === e.currentTarget)
                answerNo();
        },
    }, el('section', {
        class: 'dialog confirm-dialog',
        role: 'alertdialog',
        'aria-modal': 'true',
        'aria-labelledby': 'confirm-title',
    }, el('div', { class: 'dhead' }, el('div', { class: 'type-title', id: 'confirm-title' }, title)), el('div', { class: 'dbody' }, body), el('div', { class: 'dfoot' }, el('span', { class: 'grow' }), el('button', { class: 'btn', id: 'confirm-keep', onclick: answerNo }, force ? 'Keep the branch' : 'Keep it'), el('button', {
        class: 'btn danger',
        onclick: () => {
            call('answer_checkouts', { yes: true });
            closeLayer();
        },
    }, force ? 'Delete anyway' : 'Remove'))));
}
/** The stage's question over everything, while there is one. */
function drawConfirm(c) {
    const layer = $('layer');
    const shown = document.getElementById('checkouts-confirm');
    if (!c.confirm) {
        if (shown)
            closeLayer();
        return;
    }
    if (shown)
        return;
    layer.replaceChildren(confirmDialog(c.confirm));
    document.getElementById('confirm-keep')?.focus();
}
export function drawCheckouts() {
    if (state.rail.stage !== 'checkouts')
        return;
    const stage = $('cstage');
    const c = state.checkouts;
    if (!c) {
        stage.replaceChildren(ghosts([[0.3], [0.9], [0.85], [0.9], [0.6]]));
        return;
    }
    if (!c.repository) {
        stage.replaceChildren(empty(icon('folder'), el('div', { class: 'type-title' }, 'No repository'), el('div', { class: 'fg-muted' }, 'Choose one on the left.')));
        return;
    }
    const filterHad = document.activeElement?.id === 'checkouts-filter';
    const body = c.rows.length
        ? table(c)
        : empty(el('div', { class: 'fg-muted' }, c.filter ? `Nothing matches “${c.filter}”.` : 'No checkouts.'));
    stage.replaceChildren(head(c), body, keyBar());
    drawConfirm(c);
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
    if (action === 'newWorktree') {
        const row = state.checkouts?.rows.find((r) => r.key === state.checkouts?.selected);
        if (row && row.checkout !== null)
            nameDialog(row.branch || (row.name ?? ''));
        else
            call('new_worktree', { name: null });
        return true;
    }
    call('checkouts_action', { action });
    return true;
}
