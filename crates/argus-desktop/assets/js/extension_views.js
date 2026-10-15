// What the running extensions put in the window: their views under the
// sidebar's Extensions, each one on screen drawn with the agent table's
// renderer plus its selected row and actions — in its dock, or as a page
// of its own — the question an action asks first, their commands in the
// palette and their keys on the sheet. Rust holds all of it
// (`src/extension_views.rs`, `src/extension_keys.rs`), and `docks` puts a
// docked view's element in its dock; a row changes only when its
// extension publishes it again.
import { DOCK_ICON, dockOf, moveItems } from './docks.js';
import { $, el, icon } from './dom.js';
import { draggable } from './drag.js';
import { closeLayer, openMenu } from './menu.js';
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
    el('div', { class: 'nav-nest', role: 'group', 'aria-label': 'Extensions', hidden: !listed }, views.map((v) => draggable(el('button', {
        class: `nav nav-sub${v.on ? ' on' : ''}`,
        type: 'button',
        title: `${v.title} · from ${v.extension}${v.page ? '' : ', docked'}`,
        'aria-current': v.on ? 'page' : null,
        onclick: () => call('open_extension_view', { extension: v.extension, view: v.view }),
    }, el('span', { class: 'ic' }, viewIcon(v.icon)), el('span', { class: 'grow ttl' }, v.title), el('span', { class: 'meta' }, v.extension)), v.panel, v.title))));
}
function actionButton(view, action, kind) {
    return el('button', {
        class: kind,
        type: 'button',
        onclick: (e) => {
            e.stopPropagation();
            call('extension_action', { panel: view.panel, action: action.id });
        },
    }, action.label, action.key && [' ', keycap(action.key)]);
}
/** Moves the view to a page or back into its dock, named with its key. */
function modeButton(view, page) {
    const label = page ? 'Open as page' : 'Dock';
    const chord = state.keymap
        .find((g) => g.place === 'extension')
        ?.bindings.find((b) => b.action === (page ? 'extensionPage' : 'extensionDock'))?.chords;
    return el('button', {
        class: 'btn sm',
        type: 'button',
        onclick: () => call('extension_mode', { panel: view.panel, page }),
    }, label, chord && [' ', keycaps(chord)]);
}
/** The dock menu, as the console's head has it. */
function moveButton(view) {
    const dock = dockOf(view.panel);
    return el('button', {
        class: 'icon',
        type: 'button',
        title: 'Move to another dock',
        'aria-label': 'Move to another dock',
        'aria-haspopup': 'menu',
        onclick: (e) => {
            const box = e.currentTarget.getBoundingClientRect();
            openMenu(view.title, moveItems(view.panel, !view.page), { x: box.left, y: box.bottom + 4 });
        },
    }, icon(dock ? DOCK_ICON[dock] : 'down'));
}
function header(view) {
    const wide = view.actions.filter((a) => a.viewWide);
    const head = el('div', { class: 'vpanel-head' }, el('div', { class: 'vpanel-title' }, el('span', { class: 'type-heading grow' }, view.title), moveButton(view), !view.page &&
        el('button', {
            class: 'icon',
            type: 'button',
            title: 'Hide',
            'aria-label': `Hide ${view.title}`,
            onclick: () => call('docks', { gesture: { on: 'hide', panel: view.panel } }),
        }, icon('close'))), el('div', { class: 'vpanel-meta' }, el('span', { class: 'meta' }, `from ${view.extension}`), el('span', { class: 'vpanel-acts' }, wide.map((action) => actionButton(view, action, 'btn sm')), modeButton(view, !view.page))));
    return draggable(head, view.panel, view.title);
}
/** The view as an agent's table block: a row's status, when any has one,
 *  leads it as a chip. */
function asBlock(view) {
    const status = view.rows.some((row) => row.status !== null);
    const lead = (row) => status ? [row.status ? { kind: 'chip', chip: row.status } : { kind: 'empty' }] : [];
    return {
        kind: 'table',
        columns: [
            ...(status ? [{ title: 'Status', numeric: false, optional: false }] : []),
            ...view.columns,
        ],
        rows: view.rows.map((row) => [...lead(row), ...row.cells]),
        sorted_by: null,
        total: null,
    };
}
function body(view) {
    const perRow = view.actions.filter((a) => !a.viewWide);
    return el('div', 
    // Focusable, so a click in it gives the view the keys the sheet lists
    // under it, and a key pressed anywhere else is not the view's.
    {
        class: 'vpanel-body ext-body',
        'data-for': view.panel,
        'data-panel': view.panel,
        tabindex: '0',
    }, table(asBlock(view), {
        keys: view.rows.map((row) => row.key),
        selected: view.selected,
        select: (row) => call('select_extension_row', { panel: view.panel, key: row }),
        // The row rides along, so the default is taken on the row clicked
        // whether or not its select has landed yet.
        activate: view.default
            ? (row) => call('extension_default', { panel: view.panel, row })
            : null,
        trailing: perRow.length ? () => perRow.map((a) => actionButton(view, a, 'btn sm')) : null,
    }));
}
/** Each docked view's element, kept across redraws so `docks` can move it
 *  between its dock and the parked ones. */
const elements = new Map();
function tableOf(panel) {
    return state.extensions?.tables.find((t) => t.panel === panel) ?? null;
}
/** Whether the view is the page on the stage rather than in its dock. */
export function extensionOnStage(panel) {
    return state.rail.stage === 'extension' && tableOf(panel)?.page === true;
}
/** The element a docked view is drawn in, for `docks` to place. */
export function extensionElement(panel) {
    let section = elements.get(panel);
    if (!section) {
        section = el('section', { class: 'vpanel', 'aria-label': panel });
        elements.set(panel, section);
    }
    return section;
}
/** Draws every docked view on screen into its element, keeping the keys a
 *  click in one gave it. */
export function drawExtensionPanels() {
    const focused = focusedExtension()?.panel ?? null;
    for (const view of state.extensions?.tables ?? []) {
        if (view.page)
            continue;
        extensionElement(view.panel).replaceChildren(header(view), body(view));
    }
    if (focused)
        refocus(focused);
}
function refocus(panel) {
    document.querySelector(`.ext-body[data-panel="${CSS.escape(panel)}"]`)?.focus();
}
/** The extension view open as a page, filling the stage. */
export function drawExtensionPage() {
    if (state.rail.stage !== 'extension')
        return;
    const view = state.extensions?.tables.find((t) => t.page) ?? null;
    const page = $('xstage');
    if (!view) {
        page.replaceChildren();
        return;
    }
    const had = focusedExtension()?.panel === view.panel;
    page.replaceChildren(el('div', { class: 'xpage' }, header(view), body(view)));
    // A page has the keys the moment it is up, and keeps them across redraws.
    if (had || document.activeElement === document.body)
        refocus(view.panel);
}
/** The title of the extension view open as a page, for the strip. */
export function extensionPageTitle() {
    return state.extensions?.tables.find((t) => t.page)?.title ?? 'Extensions';
}
/** The extension view with the keys, if one has them. */
export function focusedExtension() {
    const active = document.activeElement;
    const panel = active instanceof HTMLElement
        ? active.closest('.ext-body')?.dataset.panel
        : undefined;
    return panel === undefined ? null : tableOf(panel);
}
/** Whether an extension's view has the keys. */
export function extensionViewFocused() {
    return focusedExtension() !== null;
}
/** The view the stage's keys move: the page, while one is up. */
export function pagedExtension() {
    return state.rail.stage === 'extension'
        ? (state.extensions?.tables.find((t) => t.page) ?? null)
        : null;
}
/** Says no to the action's question, as Escape or a click outside does. */
export function answerExtensionNo() {
    if (!state.extensions?.confirm)
        return false;
    call('answer_extension', { yes: false });
    closeLayer();
    return true;
}
/** The action's question, in the same dialog the Checkouts stage asks
 *  before a removal in. */
export function drawExtensionConfirm() {
    const question = state.extensions?.confirm ?? null;
    const shown = document.getElementById('extension-confirm');
    if (!question) {
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
    }, el('div', { class: 'dhead' }, el('div', { class: 'type-title', id: 'extension-confirm-title' }, `${question.label}?`)), el('div', { class: 'dbody' }, el('div', { class: 'fg-muted' }, question.question)), el('div', { class: 'dfoot' }, el('span', { class: 'meta' }, `from ${question.extension}`), el('span', { class: 'grow' }), el('button', { class: 'btn', id: 'extension-keep', onclick: answerExtensionNo }, 'Cancel'), el('button', {
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
