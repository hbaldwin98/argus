// What the running extensions put in the window beside the agents' views:
// their views as tabs of the side panel, the one out drawn with the agent
// table's renderer plus its selected row and actions, the question an action
// asks first, their commands in the palette and their keys on the sheet.
// Rust holds all of it (`src/extension_views.rs`, `src/extension_keys.rs`);
// a row changes only when its extension publishes it again.
import { $, el } from './dom.js';
import { closeLayer } from './features.js';
import { keycaps } from './keys.js';
import { state } from './state.js';
import { table } from './table.js';
import { call } from './tauri.js';
function keycap(key) {
    return el('span', { class: 'kbd' }, key);
}
/** The panel's tabs for the extensions' views listed where the person is. */
export function extensionTabs() {
    const open = state.extensions?.open ?? null;
    return (state.extensions?.views ?? []).map((v) => {
        const on = open !== null && open.extension === v.extension && open.view === v.view;
        return el('button', {
            class: `vpanel-tab${on ? ' on' : ''}`,
            role: 'tab',
            type: 'button',
            'aria-selected': String(on),
            title: `${v.title} · from ${v.extension}`,
            onclick: () => call('open_extension_view', { extension: v.extension, view: v.view }),
        }, el('span', { class: 'ttl' }, v.title));
    });
}
/** How many extension views are listed here, for the launcher's count. */
export function extensionViewCount() {
    return state.extensions?.views.length ?? 0;
}
/** Brings the first extension view out, for a launcher with no agent's. */
export function openFirstExtensionView() {
    const first = state.extensions?.views[0];
    if (!first)
        return false;
    call('open_extension_view', { extension: first.extension, view: first.view });
    return true;
}
function actionButton(action, kind) {
    return el('button', {
        class: kind,
        type: 'button',
        onclick: (e) => {
            e.stopPropagation();
            call('extension_action', { action: action.id });
        },
    }, action.label, action.key && [' ', keycap(action.key)]);
}
function header(view) {
    const wide = view.actions.filter((a) => a.viewWide);
    return el('div', { class: 'vpanel-head' }, el('div', { class: 'vpanel-title' }, el('span', { class: 'type-heading' }, view.title)), el('div', { class: 'vpanel-meta' }, el('span', { class: 'meta' }, `from ${view.extension}`), el('span', { class: 'grow' }), wide.map((action) => actionButton(action, 'btn sm'))));
}
/** The view as an agent's table block: a row's status, when any has one,
 *  leads it as a chip. */
function asBlock(view) {
    const status = view.rows.some((row) => row.status !== null);
    const lead = (row) => status ? [row.status ? { kind: 'chip', chip: row.status } : { kind: 'empty' }] : [];
    return {
        kind: 'table',
        columns: [...(status ? [{ title: 'Status', numeric: false }] : []), ...view.columns],
        rows: view.rows.map((row) => [...lead(row), ...row.cells]),
        sorted_by: null,
        total: null,
    };
}
function body(view) {
    const perRow = view.actions.filter((a) => !a.viewWide);
    const key = `extension ${view.extension}/${view.view}`;
    return el('div', 
    // Focusable, so a click in it gives the view the keys the sheet lists
    // under it, and a key pressed anywhere else is not the view's.
    { class: 'vpanel-body ext-body', 'data-for': key, tabindex: '0', id: 'extension-view' }, table(asBlock(view), {
        keys: view.rows.map((row) => row.key),
        selected: view.selected,
        select: (row) => call('select_extension_row', { key: row }),
        trailing: perRow.length ? () => perRow.map((a) => actionButton(a, 'btn sm')) : null,
    }));
}
/** The header and body of the extension view out, or nothing when an
 *  agent's view has the panel. */
export function extensionPanel() {
    const view = state.extensions?.open;
    if (!view)
        return null;
    return [header(view), body(view)];
}
/** Whether the extension view out has the keys. */
export function extensionViewFocused() {
    const view = document.getElementById('extension-view');
    return view !== null && view.contains(document.activeElement);
}
/** Says no to the action's question, as Escape or a click outside does. */
export function answerExtensionNo() {
    if (!state.extensions?.open?.confirm)
        return false;
    call('answer_extension', { yes: false });
    closeLayer();
    return true;
}
/** The action's question, in the same dialog the Checkouts stage asks
 *  before a removal in. */
export function drawExtensionConfirm() {
    const view = state.extensions?.open ?? null;
    const shown = document.getElementById('extension-confirm');
    const question = view?.confirm ?? null;
    if (!view || !question) {
        if (shown)
            closeLayer();
        return;
    }
    if (shown)
        return;
    $('layer').replaceChildren(el('div', {
        class: 'scrim',
        id: 'extension-confirm',
        onclick: (e) => {
            if (e.target === e.currentTarget)
                answerExtensionNo();
        },
    }, el('section', {
        class: 'dialog confirm-dialog',
        role: 'alertdialog',
        'aria-modal': 'true',
        'aria-labelledby': 'extension-confirm-title',
    }, el('div', { class: 'dhead' }, el('div', { class: 'type-title', id: 'extension-confirm-title' }, `${question.label}?`)), el('div', { class: 'dbody' }, el('div', { class: 'fg-muted' }, question.question)), el('div', { class: 'dfoot' }, el('span', { class: 'meta' }, `from ${view.extension}`), el('span', { class: 'grow' }), el('button', { class: 'btn', id: 'extension-keep', onclick: answerExtensionNo }, 'Cancel'), el('button', {
        class: 'btn danger',
        onclick: () => {
            call('answer_extension', { yes: true });
            closeLayer();
        },
    }, question.label)))));
    document.getElementById('extension-keep')?.focus();
}
/** The extensions' headings of the shortcuts sheet, after the keymap's. */
export function extensionSheetGroups() {
    return (state.extensions?.groups ?? []).map((group) => el('section', { class: 'sheet-group' }, el('div', { class: 'mlabel' }, group.title), group.bindings.map((binding) => el('div', { class: 'sheet-row' }, el('span', { class: 'grow' }, binding.label), keycaps(binding.chords)))));
}
/** Runs an extension's command with where the person is; the rail's picked
 *  row is the page's to say. */
export function runExtensionCommand(extension, command) {
    const pick = state.railPick;
    call('extension_command', {
        extension,
        command,
        checkout: pick?.checkout ?? null,
        pane: pick?.pane ?? null,
    });
}
/** The palette's results for every extension command and view. */
export function extensionResults() {
    const page = state.extensions;
    if (!page)
        return [];
    return [
        ...page.commands.map((c) => ({
            group: 'Commands',
            key: `extension ${c.extension} ${c.command}`,
            text: c.title,
            detail: c.extension,
            aside: c.chords.length ? keycaps(c.chords) : null,
            choose: () => runExtensionCommand(c.extension, c.command),
        })),
        ...page.views.map((v) => ({
            group: 'Commands',
            key: `extension view ${v.extension} ${v.view}`,
            text: `Show ${v.title}`,
            detail: v.extension,
            aside: null,
            choose: () => call('open_extension_view', { extension: v.extension, view: v.view }),
        })),
    ];
}
