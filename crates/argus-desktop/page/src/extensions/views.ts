// What the running extensions put in the window: their views under the
// sidebar's Extensions, each one on screen drawn with the agent table's
// renderer plus its selected row and actions — in its dock, or as a page
// of its own — the question an action asks first, their commands in the
// palette and their keys on the sheet. Rust holds all of it
// (`src/extensions/views.rs`, `src/extensions/keys.rs`), and `docks` puts
// a docked view's element in its dock; a row changes only when its
// extension publishes it again.

import type { ExtConsoleEnd } from '../bindings/ExtConsoleEnd';
import type { ExtensionGesture } from '../bindings/ExtensionGesture';
import type { ExtTable } from '../bindings/ExtTable';
import type { ExtTableAction } from '../bindings/ExtTableAction';
import type { ExtTableConsole } from '../bindings/ExtTableConsole';
import type { Panel } from '../bindings/Panel';
import type { Value } from '../bindings/Value';
import { dockMenuButton, dockShowsTabs } from '../layout/docks.js';
import { $, closeButton, el, escKey, focusQuietly, icon, type IconName } from '../kit/dom.js';
import { draggable } from '../layout/drag.js';
import { closeLayer } from '../kit/menu.js';
import { keycaps } from '../keys/chords.js';
import { bindView, keyView, type Screen, screen, viewScreen } from '../panes/screen.js';
import { state } from '../state.js';
import { table, type TableBlock } from '../views/blocks/table.js';
import { call } from '../connection/tauri.js';

/** A gesture in the view `panel` names. */
export function extensionGesture(panel: Panel, gesture: ExtensionGesture): void {
  call('extension', { panel, gesture });
}

function keycap(key: string): HTMLElement {
  return el('span', { class: 'kbd' }, key);
}

/** The icon an extension named, drawn by the page; a name it does not draw
 *  is the generic one, and an extension never sends an image. */
const ICONS: Record<string, IconName> = {
  box: 'box',
  server: 'server',
  database: 'database',
  globe: 'globe',
  list: 'list',
  rocket: 'rocket',
  terminal: 'console',
  gear: 'gear',
};

function viewIcon(name: string | null): Element {
  // Own keys only: `constructor` is a name the protocol lets through, and
  // inherited it is no icon at all.
  return icon((name !== null && Object.hasOwn(ICONS, name) && ICONS[name]) || 'puzzle');
}

/** The sidebar's Extensions: a fold after the built-in stages listing
 *  every view whose scope is where the person is. Nothing listed, it is
 *  not drawn, as the rail's fold of quiet repositories is not. */
export function drawExtensionNav(): void {
  const page = state.extensions;
  const views = page?.views ?? [];
  const holder = $('nav-extensions');
  if (views.length === 0) {
    holder.replaceChildren();
    return;
  }
  const listed = page?.listed ?? false;
  holder.replaceChildren(
    el(
      'button',
      {
        class: 'nav nav-fold',
        type: 'button',
        'aria-expanded': String(listed),
        onclick: () => call('toggle_extensions', {}),
      },
      el('span', { class: 'ic' }, icon(listed ? 'down' : 'right')),
      el('span', { class: 'grow' }, 'Extensions'),
      el('span', { class: 'meta' }, String(views.length)),
    ),
    // Folded, the views are kept but hidden, so a fold is one redraw.
    el(
      'div',
      { class: 'nav-nest', role: 'group', 'aria-label': 'Extensions', hidden: !listed },
      views.map((v) =>
        draggable(
          el(
            'button',
            {
              class: `nav nav-sub${v.on ? ' on' : ''}`,
              type: 'button',
              title: `${v.title} · from ${v.extension}${v.page ? '' : ', docked'}`,
              'aria-current': v.on ? 'page' : null,
              onclick: () => call('open_extension_view', { extension: v.extension, view: v.view }),
            },
            el('span', { class: 'ic' }, viewIcon(v.icon)),
            el('span', { class: 'grow ttl' }, v.title),
            el('span', { class: 'meta' }, v.extension),
          ),
          v.panel,
          v.title,
        ),
      ),
    ),
  );
}

function actionButton(view: ExtTable, action: ExtTableAction, kind: string): HTMLElement {
  return el(
    'button',
    {
      class: kind,
      type: 'button',
      onclick: (e: MouseEvent) => {
        e.stopPropagation();
        extensionGesture(view.panel, { on: 'act', action: action.id });
      },
    },
    action.label,
    action.key && [' ', keycap(action.key)],
  );
}

/** Moves the view to a page or back into its dock, named with its key. */
function modeButton(view: ExtTable, page: boolean): HTMLElement {
  const label = page ? 'Open as page' : 'Dock';
  const chord = state.keymap
    .find((g) => g.place === 'extension')
    ?.bindings.find((b) => b.action === (page ? 'extensionPage' : 'extensionDock'))?.chords;
  return el(
    'button',
    {
      class: 'btn sm',
      type: 'button',
      onclick: () => call('extension_mode', { panel: view.panel, page }),
    },
    label,
    chord && [' ', keycaps(chord)],
  );
}

function header(view: ExtTable): HTMLElement {
  // An ended console's actions are in the strip beside how it ended.
  const wide = view.actions.filter((a) => a.viewWide && !view.console?.ended);
  const head = el(
    'div',
    { class: 'vpanel-head' },
    // As a page the strip already names the view; docked it names itself.
    // A dock's tab strip names a docked view too, and holds its dock menu.
    !view.page &&
      !dockShowsTabs(view.panel) &&
      el(
        'div',
        { class: 'vpanel-title' },
        el('span', { class: 'type-heading grow' }, view.title),
        dockMenuButton(view.panel, view.title, true),
        el(
          'button',
          {
            class: 'icon',
            type: 'button',
            title: 'Hide',
            'aria-label': `Hide ${view.title}`,
            onclick: () => call('docks', { gesture: { on: 'hide', panel: view.panel } }),
          },
          icon('close'),
        ),
      ),
    el(
      'div',
      { class: 'vpanel-meta' },
      el('span', { class: 'meta' }, `from ${view.extension}`),
      el(
        'span',
        { class: 'vpanel-acts' },
        wide.map((action) => actionButton(view, action, 'btn sm')),
        modeButton(view, !view.page),
      ),
    ),
  );
  // As a page the row is only its buttons; its Dock button is the way back.
  return view.page ? head : draggable(head, view.panel, view.title);
}

/** The view as an agent's table block: a row's status, when any has one,
 *  leads it as a chip. */
function asBlock(view: ExtTable): TableBlock {
  const status = view.rows.some((row) => row.status !== null);
  const lead = (row: ExtTable['rows'][number]): Value[] =>
    status ? [row.status ? { kind: 'chip', chip: row.status } : { kind: 'empty' }] : [];
  return {
    kind: 'table',
    row_keys: [],
    select: 'none',
    actions: [],
    details: [],
    columns: [
      ...(status ? [{ title: 'Status', numeric: false, optional: false }] : []),
      ...view.columns,
    ],
    rows: view.rows.map((row) => [...lead(row), ...row.cells]),
    sorted_by: null,
    total: null,
  };
}

/** A console view's body, kept across redraws: the terminal in it holds the
 *  keys' text area, and a node taken out of the page loses its focus. */
interface ConsoleBody {
  body: HTMLElement;
  strip: HTMLElement;
  screen: Screen;
}
const consoles = new Map<Panel, ConsoleBody>();

function endedWords(end: ExtConsoleEnd): string {
  return end.code === null ? 'Ended by a signal' : `Ended (exit ${end.code})`;
}

/** The view as a terminal on the screen Rust gave it, under a strip that
 *  says it is starting or has ended and offers the view's own actions. */
function consoleBody(view: ExtTable, info: ExtTableConsole): HTMLElement {
  let c = consoles.get(view.panel);
  if (!c) {
    const terminal = el('div', { class: 'terminal ext-term' });
    const strip = el('div', { class: 'ext-strip', hidden: true });
    const shell = el('div', { class: 'vpanel-body ext-console' }, strip, terminal);
    c = { body: shell, strip, screen: viewScreen(terminal) };
    consoles.set(view.panel, c);
  }
  bindView(c.screen, info.side);
  c.screen.expect(info.pane);
  c.strip.hidden = info.pane !== null && info.ended === null;
  c.strip.replaceChildren(
    ...(info.pane === null
      ? [el('span', { class: 'fg-muted' }, 'Starting…')]
      : info.ended
        ? [
            el('span', { class: 'fg-muted grow' }, endedWords(info.ended)),
            ...view.actions.filter((a) => a.viewWide).map((a) => actionButton(view, a, 'btn sm')),
          ]
        : []),
  );
  return c.body;
}

/** Keeps the keys' text area out of what is about to be thrown away: it
 *  sits in a console view's terminal while that has the keys, and the page
 *  looks it up by id. `placeTerminal` puts it where it belongs after. */
function rescueInput(from: HTMLElement): void {
  const input = document.getElementById('input');
  if (input && from.contains(input)) screen.terminal.append(input);
}

/** Puts a view's header and body in `parent`. A console's body stays where
 *  it is when it is there already, and only the header is replaced. */
function place(parent: HTMLElement, head: HTMLElement, content: HTMLElement): void {
  if (content.parentElement === parent && parent.lastElementChild === content) {
    parent.firstElementChild?.replaceWith(head);
  } else {
    rescueInput(parent);
    parent.replaceChildren(head, content);
  }
}

/** The console view with the keys is the screen Rust's keys go through, and
 *  a view newly given them takes the text area's focus. */
let keyedPanel: Panel | null = null;

function syncKeyedConsole(): void {
  const keyed = state.extensions?.tables.find((t) => t.console?.keyed) ?? null;
  const c = keyed ? (consoles.get(keyed.panel) ?? null) : null;
  keyView(c?.screen ?? null);
  const next = c && keyed ? keyed.panel : null;
  const fresh = next !== null && next !== keyedPanel;
  keyedPanel = next;
  // A body moved from a dock to the stage left the focus on the page.
  if (next !== null && (fresh || document.activeElement === document.body)) $('input').focus();
}

/** A console view that is no longer on screen draws nowhere. */
function releaseConsoles(): void {
  const on = new Set(state.extensions?.tables.filter((t) => t.console).map((t) => t.panel));
  for (const [panel, c] of consoles) if (!on.has(panel)) bindView(c.screen, null);
}

function body(view: ExtTable): HTMLElement {
  if (view.console) return consoleBody(view, view.console);
  const perRow = view.actions.filter((a) => !a.viewWide);
  return el(
    'div',
    // Focusable, so a click in it gives the view the keys the sheet lists
    // under it, and a key pressed anywhere else is not the view's.
    {
      class: 'vpanel-body ext-body',
      'data-for': view.panel,
      'data-panel': view.panel,
      tabindex: '0',
    },
    table(asBlock(view), {
      keys: view.rows.map((row) => row.key),
      selected: view.selected,
      select: (row) => extensionGesture(view.panel, { on: 'select', key: row }),
      // The row rides along, so the default is taken on the row clicked
      // whether or not its select has landed yet.
      activate: view.default ? (row) => extensionGesture(view.panel, { on: 'default', row }) : null,
      trailing: perRow.length ? () => perRow.map((a) => actionButton(view, a, 'btn sm')) : null,
    }),
  );
}

/** Each docked view's element, kept across redraws so `docks` can move it
 *  between its dock and the parked ones. */
const elements = new Map<Panel, HTMLElement>();

function tableOf(panel: Panel): ExtTable | null {
  return state.extensions?.tables.find((t) => t.panel === panel) ?? null;
}

/** Whether the view is the page on the stage rather than in its dock. */
export function extensionOnStage(panel: Panel): boolean {
  return state.rail.stage === 'extension' && tableOf(panel)?.page === true;
}

/** The element a docked view is drawn in, for `docks` to place. */
export function extensionElement(panel: Panel): HTMLElement {
  let section = elements.get(panel);
  if (!section) {
    section = el('section', { class: 'vpanel', 'aria-label': panel });
    elements.set(panel, section);
  }
  return section;
}

/** Draws every docked view on screen into its element, keeping the keys a
 *  click in one gave it. */
export function drawExtensionPanels(): void {
  const focused = focusedExtension()?.panel ?? null;
  for (const view of state.extensions?.tables ?? []) {
    if (view.page) continue;
    place(extensionElement(view.panel), header(view), body(view));
  }
  releaseConsoles();
  syncKeyedConsole();
  if (focused) refocus(focused);
}

function refocus(panel: Panel): void {
  document.querySelector<HTMLElement>(`.ext-body[data-panel="${CSS.escape(panel)}"]`)?.focus();
}

/** The extension view open as a page, filling the stage. */
export function drawExtensionPage(): void {
  if (state.rail.stage !== 'extension') return;
  const view = state.extensions?.tables.find((t) => t.page) ?? null;
  const page = $('xstage');
  if (!view) {
    rescueInput(page);
    page.replaceChildren();
    return;
  }
  const had = focusedExtension()?.panel === view.panel;
  const wrap = page.firstElementChild;
  const content = body(view);
  if (
    wrap instanceof HTMLElement &&
    wrap.classList.contains('xpage') &&
    content.parentElement === wrap
  ) {
    place(wrap, header(view), content);
  } else {
    rescueInput(page);
    page.replaceChildren(el('div', { class: 'xpage' }, header(view), content));
  }
  syncKeyedConsole();
  // A page has the keys the moment it is up, and keeps them across redraws.
  if (had || document.activeElement === document.body) refocus(view.panel);
}

/** The title of the extension view open as a page, for the strip. */
export function extensionPageTitle(): string {
  return state.extensions?.tables.find((t) => t.page)?.title ?? 'Extensions';
}

/** The extension view with the keys, if one has them. */
export function focusedExtension(): ExtTable | null {
  const active = document.activeElement;
  const panel =
    active instanceof HTMLElement
      ? active.closest<HTMLElement>('.ext-body')?.dataset.panel
      : undefined;
  return panel === undefined ? null : tableOf(panel);
}

/** Whether an extension's view has the keys. */
export function extensionViewFocused(): boolean {
  return focusedExtension() !== null;
}

/** The view the stage's keys move: the page, while one is up. */
export function pagedExtension(): ExtTable | null {
  return state.rail.stage === 'extension'
    ? (state.extensions?.tables.find((t) => t.page) ?? null)
    : null;
}

/** Says no to the action's question, as Escape or a click outside does. */
export function answerExtensionNo(): boolean {
  if (!state.extensions?.confirm) return false;
  call('answer_extension', { yes: false });
  closeLayer();
  return true;
}

/** The action's question, in the same dialog the Checkouts stage asks
 *  before a removal in. */
export function drawExtensionConfirm(): void {
  const question = state.extensions?.confirm ?? null;
  const shown = document.getElementById('extension-confirm');
  if (!question) {
    if (shown) closeLayer();
    return;
  }
  if (shown) return;
  $('layer').replaceChildren(
    el(
      'div',
      {
        class: 'scrim',
        id: 'extension-confirm',
        onclick: (e: MouseEvent) => {
          if (e.target === e.currentTarget) answerExtensionNo();
        },
      },
      el(
        'section',
        {
          class: 'dialog confirm-dialog',
          role: 'alertdialog',
          'aria-modal': 'true',
          'aria-labelledby': 'extension-confirm-title',
        },
        el(
          'div',
          { class: 'dhead' },
          el(
            'div',
            { class: 'type-title grow', id: 'extension-confirm-title' },
            `${question.label}?`,
          ),
          closeButton(answerExtensionNo),
        ),
        el('div', { class: 'dbody prose' }, el('div', { class: 'fg-muted' }, question.question)),
        el(
          'div',
          { class: 'dfoot' },
          el('span', { class: 'dwhy grow' }, `from ${question.extension}`),
          el(
            'button',
            { class: 'btn', id: 'extension-keep', onclick: answerExtensionNo },
            'Cancel ',
            escKey(),
          ),
          el(
            'button',
            {
              class: 'btn danger',
              onclick: () => {
                call('answer_extension', { yes: true });
                closeLayer();
              },
            },
            question.label,
          ),
        ),
      ),
    ),
  );
  focusQuietly(document.getElementById('extension-keep'));
}

/** The extensions' headings of the shortcuts sheet, after the keymap's. */
export function extensionSheetGroups(): HTMLElement[] {
  return (state.extensions?.groups ?? []).map((group) =>
    el(
      'section',
      { class: 'sheet-group' },
      el('div', { class: 'mlabel' }, group.title),
      group.bindings.map((binding) =>
        el(
          'div',
          { class: 'sheet-row' },
          el('span', { class: 'grow' }, binding.label),
          keycaps(binding.chords),
        ),
      ),
    ),
  );
}

/** Runs an extension's command with where the person is; the rail's picked
 *  row is the page's to say. */
export function runExtensionCommand(extension: string, command: string): void {
  const pick = state.railPick;
  call('extension_command', {
    extension,
    command,
    checkout: pick?.checkout ?? null,
    pane: pick?.pane ?? null,
  });
}

/** The palette's results for every extension command and view. */
export function extensionResults(): {
  group: 'Commands';
  key: string;
  text: string;
  detail?: string;
  aside: HTMLElement | null;
  choose: () => void;
}[] {
  const page = state.extensions;
  if (!page) return [];
  return [
    ...page.views.map((v) => ({
      group: 'Commands' as const,
      key: `extension go ${v.extension} ${v.view}`,
      text: `Go to ${v.title}`,
      detail: v.extension,
      aside: null,
      choose: () => call('open_extension_view', { extension: v.extension, view: v.view }),
    })),
    ...page.commands.map((c) => ({
      group: 'Commands' as const,
      key: `extension ${c.extension} ${c.command}`,
      text: c.title,
      detail: c.extension,
      aside: c.chords.length ? keycaps(c.chords) : null,
      choose: () => runExtensionCommand(c.extension, c.command),
    })),
  ];
}
