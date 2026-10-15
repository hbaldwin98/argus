// What the running extensions put in the window: their views under the
// sidebar's Extensions, the one open drawn with the agent table's renderer
// plus its selected row and actions — docked in the side panel beside the
// agents' views, or as a page of its own — the question an action asks
// first, their commands in the palette and their keys on the sheet. Rust
// holds all of it (`src/extension_views.rs`, `src/extension_keys.rs`); a
// row changes only when its extension publishes it again.
import { $, el, icon } from './dom.js';
import { closeLayer } from './menu.js';
import { keycaps } from './keys.js';
import { state } from './state.js';
import { table } from './table.js';
import { call } from './tauri.js';
function keycap(key) {
    return el('span', { class: 'kbd' }, key);
}
/** The icon an extension named, drawn by the page; a name it does not draw
 *  is the generic one, and an extension never sends an image. */
const ICONS = {
    box: 'box',
    server: 'server',
    database: 'database',
    globe: 'globe',
    list: 'list',
    rocket: 'rocket',
    terminal: 'console',
    gear: 'gear',
};
function viewIcon(name) {
    // Own keys only: `constructor` is a name the protocol lets through, and
    // inherited it is no icon at all.
    return icon((name !== null && Object.hasOwn(ICONS, name) && ICONS[name]) || 'puzzle');
}
/** The sidebar's Extensions: a fold after the built-in stages listing
 *  every view whose scope is where the person is. Nothing listed, it is
 *  not drawn, as the rail's fold of quiet repositories is not. */
export function drawExtensionNav() {
    const page = state.extensions;
    const views = page?.views ?? [];
    const holder = $('nav-extensions');
    if (views.length === 0) {
        holder.replaceChildren();
        return;
    }
    const listed = page?.listed ?? false;
    holder.replaceChildren(el('button', {
        class: 'nav nav-fold',
        type: 'button',
        'aria-expanded': String(listed),
        onclick: () => call('toggle_extensions', {}),
    }, el('span', { class: 'ic' }, icon(listed ? 'down' : 'right')), el('span', { class: 'grow' }, 'Extensions'), el('span', { class: 'meta' }, String(views.length))), 
    // Folded, the views are kept but hidden, so a fold is one redraw.
    el('div', { class: 'nav-nest', role: 'group', 'aria-label': 'Extensions', hidden: !listed }, views.map((v) => el('button', {
        class: `nav nav-sub${v.on ? ' on' : ''}`,
        type: 'button',
        title: `${v.title} · from ${v.extension}${v.page ? '' : ', docked'}`,
        'aria-current': v.on ? 'page' : null,
        onclick: () => call('open_extension_view', { extension: v.extension, view: v.view }),
    }, el('span', { class: 'ic' }, viewIcon(v.icon)), el('span', { class: 'grow ttl' }, v.title), el('span', { class: 'meta' }, v.extension)))));
}
/** The panel's tabs for the extension views that open docked. */
export function extensionTabs() {
    return (state.extensions?.views ?? [])
        .filter((v) => !v.page)
        .map((v) => el('button', {
        class: `vpanel-tab${v.on ? ' on' : ''}`,
        role: 'tab',
        type: 'button',
        'aria-selected': String(v.on),
        title: `${v.title} · from ${v.extension}`,
        onclick: () => call('open_extension_view', { extension: v.extension, view: v.view }),
    }, el('span', { class: 'ttl' }, v.title)));
}
/** How many docked extension views are listed here, for the launcher. */
export function extensionViewCount() {
    return (state.extensions?.views ?? []).filter((v) => !v.page).length;
}
/** Brings the first docked extension view out, for a launcher with no
 *  agent's. */
export function openFirstExtensionView() {
    const first = (state.extensions?.views ?? []).find((v) => !v.page);
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
/** Moves the view to a page or docks it, named with its key. */
function modeButton(page) {
    const label = page ? 'Open as page' : 'Dock to side panel';
    const chord = state.keymap
        .find((g) => g.place === 'extension')
        ?.bindings.find((b) => b.action === (page ? 'extensionPage' : 'extensionDock'))?.chords;
    return el('button', { class: 'btn sm', type: 'button', onclick: () => call('extension_mode', { page }) }, label, chord && [' ', keycaps(chord)]);
}
function header(view) {
    const wide = view.actions.filter((a) => a.viewWide);
    return el('div', { class: 'vpanel-head' }, el('div', { class: 'vpanel-title' }, el('span', { class: 'type-heading' }, view.title)), el('div', { class: 'vpanel-meta' }, el('span', { class: 'meta' }, `from ${view.extension}`), el('span', { class: 'grow' }), wide.map((action) => actionButton(action, 'btn sm')), modeButton(!view.page)));
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
/** The header and body of the extension view docked, or nothing when an
 *  agent's view has the panel. */
export function extensionPanel() {
    const view = state.extensions?.open;
    if (!view || view.page)
        return null;
    return [header(view), body(view)];
}
/** The extension view open as a page, filling the stage. */
export function drawExtensionPage() {
    if (state.rail.stage !== 'extension')
        return;
    const view = state.extensions?.open;
    const page = $('xstage');
    if (!view || !view.page) {
        page.replaceChildren();
        return;
    }
    const had = extensionViewFocused();
    page.replaceChildren(el('div', { class: 'xpage' }, header(view), body(view)));
    // A page has the keys the moment it is up, and keeps them across redraws.
    if (had || document.activeElement === document.body)
        document.getElementById('extension-view')?.focus();
}
/** The title of the extension view open as a page, for the strip. */
export function extensionPageTitle() {
    const view = state.extensions?.open;
    return view?.page ? view.title : 'Extensions';
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
        ...page.views.map((v) => ({
            group: 'Commands',
            key: `extension go ${v.extension} ${v.view}`,
            text: `Go to ${v.title}`,
            detail: v.extension,
            aside: null,
            choose: () => call('open_extension_view', { extension: v.extension, view: v.view }),
        })),
        ...page.commands.map((c) => ({
            group: 'Commands',
            key: `extension ${c.extension} ${c.command}`,
            text: c.title,
            detail: c.extension,
            aside: c.chords.length ? keycaps(c.chords) : null,
            choose: () => runExtensionCommand(c.extension, c.command),
        })),
    ];
}
