// The saved views: the panel each opens in, with its trail of columns side
// by side, and the sidebar's Views section that lists them. Rust holds the
// list, which panels are open and each one's columns; this draws them and
// says what was pressed. A column is one saved view under a header with its
// scope, how stale it is and what refreshes it, so a refresh, a stale mark
// and a delete go to the daemon and the column changes only when it says.

import type { PageSaved } from './bindings/PageSaved';
import type { Panel } from './bindings/Panel';
import type { SavedPanel } from './bindings/SavedPanel';
import type { ViewCtx } from './viewctx.js';
import { actionButton } from './actions.js';
import { blocks } from './blocks.js';
import { chip } from './chip.js';
import { DOCK_ICON, dockOf, moveItems } from './docks.js';
import { closesOnMiddle } from './middle.js';
import { draggable } from './drag.js';
import { $, el, icon } from './dom.js';
import { openMenu } from './menu.js';
import { savedCtx } from './saved_ctx.js';
import { count, grouped, nested, type Nested } from './saved_logic.js';
import { savedView } from './savedlink.js';
import { state } from './state.js';
import { call } from './tauri.js';
import { ago } from './values.js';
import { viewLink } from './viewlink.js';

/** The delete questions open, by panel, column and view: a panel is redrawn
 *  whole whenever anything in it changes, and a question being answered must
 *  not vanish. The page's own, like a tab chosen. */
const asking = new Set<string>();
/** The columns whose metadata is unfolded, by the same key. */
const unfolded = new Set<string>();
/** How many columns each panel had when last drawn, so one that grew
 *  scrolls to show the new column. */
const counts = new Map<string, number>();
/** Each panel's element, kept across redraws so `docks` can move it
 *  between its dock and the parked ones. */
const elements = new Map<string, HTMLElement>();

/** The element a saved panel is drawn in, for `docks` to place. */
export function savedElement(panel: Panel): HTMLElement {
  let section = elements.get(panel);
  if (!section) {
    section = el('section', { class: 'vpanel saved-panel', 'aria-label': 'Saved view' });
    elements.set(panel, section);
  }
  return section;
}

/** Draws every saved panel the docks hold. */
export function drawSavedPanels(): void {
  const placed = new Set<string>();
  for (const panel of state.saved?.panels ?? []) {
    placed.add(panel.panel);
    fill(panel);
  }
  for (const name of elements.keys()) {
    if (!placed.has(name)) {
      elements.delete(name);
      counts.delete(name);
    }
  }
}

function fill(p: SavedPanel): void {
  const section = savedElement(p.panel);
  const cols = section.querySelector<HTMLElement>('.saved-cols');
  const left = cols?.scrollLeft ?? 0;
  const tops = new Map<string, number>();
  for (const body of section.querySelectorAll<HTMLElement>('.vpanel-body[data-for]')) {
    tops.set(body.getAttribute('data-for') ?? '', body.scrollTop);
  }
  const fresh = el(
    'div',
    { class: 'saved-cols' },
    p.columns.map((id, i) => column(p, i, id)),
  );
  section.replaceChildren(panelHead(p), fresh);
  for (const body of fresh.querySelectorAll<HTMLElement>('.vpanel-body[data-for]')) {
    body.scrollTop = tops.get(body.getAttribute('data-for') ?? '') ?? 0;
  }
  const grew = p.columns.length > (counts.get(p.panel) ?? p.columns.length);
  counts.set(p.panel, p.columns.length);
  fresh.scrollLeft = left;
  if (grew) {
    const toEnd = (): void => {
      fresh.scrollLeft = fresh.scrollWidth;
    };
    toEnd();
    // A panel just brought out of the parked ones has no width until it is
    // laid out.
    requestAnimationFrame(toEnd);
  }
}

/** The strip across the panel: draggable like any panel's header, with the
 *  dock menu and the panel's close, which takes it out of its dock with its
 *  columns rather than hiding the dock and every tab in it. */
function panelHead(p: SavedPanel): HTMLElement {
  const first = savedView(p.columns[0] ?? '');
  const title = first?.view.title ?? 'Saved view';
  const close = (): void => call('close_saved_column', { panel: p.panel, column: 0 });
  const strip = el(
    'div',
    { class: 'vpanel-tabs saved-head', role: 'toolbar', 'aria-label': 'Saved view' },
    el('span', { class: 'ttl', title }, title),
    p.columns.length > 1 && el('span', { class: 'meta' }, `${p.columns.length} columns`),
    el('span', { class: 'grow' }),
    el(
      'button',
      {
        class: 'icon',
        type: 'button',
        title: 'Move to another dock',
        'aria-label': 'Move to another dock',
        'aria-haspopup': 'menu',
        onclick: (e: MouseEvent) => {
          const box = (e.currentTarget as HTMLElement).getBoundingClientRect();
          openMenu('Dock the view', moveItems(p.panel, true), { x: box.left, y: box.bottom + 4 });
        },
      },
      icon(DOCK_ICON[dockOf(p.panel) ?? 'right']),
    ),
    el(
      'button',
      {
        class: 'icon',
        type: 'button',
        title: 'Close',
        'aria-label': `Close ${title}`,
        onclick: close,
      },
      icon('close'),
    ),
  );
  return closesOnMiddle(draggable(strip, p.panel, title), close);
}

function column(p: SavedPanel, i: number, id: string): HTMLElement {
  const view = savedView(id);
  if (!view) return missing(p, i, id);
  const host = { panel: p.panel, column: i };
  const ctx = savedCtx(view, host, [id], 0, `${p.panel}/${i}`);
  return el(
    'section',
    { class: `saved-col${view.stale ? ' stale' : ''}`, 'aria-label': view.view.title },
    header(p, i, view, ctx),
    el('div', { class: 'vpanel-body', 'data-for': `${i}|${id}` }, blocks(view.view.blocks, ctx)),
  );
}

/** A column whose view the window no longer holds, or does not yet. */
function missing(p: SavedPanel, i: number, id: string): HTMLElement {
  return el(
    'section',
    { class: 'saved-col', 'aria-label': 'View not found' },
    el(
      'div',
      { class: 'vpanel-head' },
      el(
        'div',
        { class: 'vpanel-title' },
        el('span', { class: 'type-heading grow' }, 'View not found'),
        closeButton(p, i),
      ),
      el('div', { class: 'vpanel-meta' }, el('span', { class: 'meta' }, id)),
    ),
  );
}

function closeButton(p: SavedPanel, i: number): HTMLElement {
  return el(
    'button',
    {
      class: 'icon',
      type: 'button',
      title: i === 0 ? 'Close this view' : 'Close this column',
      'aria-label': i === 0 ? 'Close this view' : 'Close this column',
      onclick: () => call('close_saved_column', { panel: p.panel, column: i }),
    },
    icon('close'),
  );
}

function columnKey(p: SavedPanel, i: number, view: PageSaved): string {
  return `${p.panel}|${i}|${view.id}`;
}

function header(p: SavedPanel, i: number, view: PageSaved, ctx: ViewCtx): HTMLElement {
  const k = columnKey(p, i, view);
  const open = unfolded.has(k);
  return el(
    'div',
    { class: 'vpanel-head' },
    closesOnMiddle(
      el(
        'div',
        { class: 'vpanel-title' },
        el('span', { class: 'type-heading grow' }, view.view.title),
        view.view.status && chip(view.view.status),
        view.stale && chip({ label: 'stale', tone: 'warn' }),
        i > 0 &&
          el(
            'button',
            {
              class: 'icon',
              type: 'button',
              title: 'Open as a dialog',
              'aria-label': 'Open as a dialog',
              onclick: () => call('float_saved_column', { panel: p.panel, column: i }),
            },
            icon('box'),
          ),
        i > 0 &&
          el(
            'button',
            {
              class: 'icon',
              type: 'button',
              title: 'Pop out into a panel of its own',
              'aria-label': 'Pop out into a panel of its own',
              onclick: () => call('pop_out_saved_column', { panel: p.panel, column: i }),
            },
            icon('expand'),
          ),
        closeButton(p, i),
      ),
      () => call('close_saved_column', { panel: p.panel, column: i }),
    ),
    el(
      'div',
      { class: 'vpanel-meta' },
      el('span', { class: 'meta' }, view.scope.label),
      el(
        'span',
        { class: 'meta', title: builtAt(view).toLocaleString() },
        `built ${ago(builtAt(view).toISOString())}`,
        view.builtCommit ? ` at ${view.builtCommit.slice(0, 7)}` : '',
        view.refreshing ? ' · asked to build it again' : '',
      ),
      el(
        'span',
        { class: 'vpanel-acts' },
        view.view.actions.map((a) => actionButton(a, ctx)),
        el(
          'button',
          {
            class: 'btn sm',
            type: 'button',
            disabled: view.refreshing,
            title: 'Ask the agent that saved it to build it again',
            onclick: () => call('refresh_saved_view', { id: view.id }),
          },
          'Refresh',
        ),
        el(
          'button',
          {
            class: 'btn sm',
            type: 'button',
            disabled: view.stale,
            title: 'Say it is out of date',
            onclick: () => call('mark_saved_view_stale', { id: view.id }),
          },
          'Mark stale',
        ),
        deleteControl(k, view),
        el(
          'button',
          {
            class: 'btn sm',
            type: 'button',
            'aria-expanded': String(open),
            title: 'What it was built from',
            onclick: () => {
              if (open) unfolded.delete(k);
              else unfolded.add(k);
              drawSavedPanels();
            },
          },
          'Details',
        ),
      ),
    ),
    open && metadata(view),
  );
}

function builtAt(view: PageSaved): Date {
  return new Date(Number(view.builtAt) * 1000);
}

/** Delete asks first: it is for every window, and the agent that built the
 *  view would have to build it again. */
function deleteControl(k: string, view: PageSaved): HTMLElement {
  if (!asking.has(k)) {
    return el(
      'button',
      {
        class: 'btn sm vtone-bad',
        type: 'button',
        title: 'Delete this saved view for every window',
        onclick: () => {
          asking.add(k);
          drawSavedPanels();
        },
      },
      'Delete',
    );
  }
  return el(
    'span',
    { class: 'vconfirm', role: 'group', 'aria-label': 'Delete this saved view?' },
    el('span', { class: 'vconfirm-q' }, 'Delete it?'),
    el(
      'button',
      {
        class: 'btn sm vtone-bad',
        type: 'button',
        onclick: () => {
          asking.delete(k);
          call('delete_saved_view', { id: view.id });
          drawSavedPanels();
        },
      },
      'Yes',
    ),
    el(
      'button',
      {
        class: 'btn sm',
        type: 'button',
        onclick: () => {
          asking.delete(k);
          drawSavedPanels();
        },
      },
      'No',
    ),
  );
}

function row(label: string, ...value: (Node | string | false)[]): HTMLElement {
  return el('div', { class: 'saved-meta-row' }, el('dt', null, label), el('dd', null, value));
}

/** What an agent needs to build the view again, as it was saved. */
function metadata(view: PageSaved): HTMLElement {
  return el(
    'dl',
    { class: 'saved-meta' },
    row('Scope', view.scope.label),
    row(
      'Built',
      `${builtAt(view).toLocaleString()}`,
      view.builtCommit ? ` · commit ${view.builtCommit.slice(0, 12)}` : '',
    ),
    view.features.length > 0 &&
      row('Features', view.features.map((f) => `#${f.number}`).join(', ')),
    view.recipe ? row('Recipe', el('span', { class: 'saved-recipe' }, view.recipe)) : false,
    view.sources.length > 0 &&
      row(
        'Sources',
        el(
          'ul',
          { class: 'saved-sources' },
          view.sources.map((s) =>
            el(
              'li',
              null,
              el('span', { class: 'meta' }, `${s.kind} `),
              s.kind === 'url' ? viewLink(s.text, s.text) : s.text,
            ),
          ),
        ),
      ),
  );
}

// ---- The sidebar's Views ----------------------------------------------------

/** Whether the section is unfolded, and whether it lists only the views
 *  about where the person is. The page's own. */
let listed = true;
let hereOnly = true;
/** The views whose linked views are unfolded under them, by id. */
const navOpen = new Set<string>();

/** The sidebar's Views: a fold after the Extensions listing the saved
 *  views grouped by scope, those about where the person is by default. With
 *  none saved it is not drawn. */
export function drawSavedNav(): void {
  const holder = $('nav-saved');
  const all = state.saved?.views ?? [];
  if (all.length === 0) {
    holder.replaceChildren();
    return;
  }
  const open = new Set(
    (state.saved?.panels ?? [])
      .filter((p) => state.docks && shownIn(p.panel))
      .flatMap((p) => p.columns),
  );
  const groups = grouped(all, hereOnly);
  holder.replaceChildren(
    el(
      'button',
      {
        class: 'nav nav-fold',
        type: 'button',
        'aria-expanded': String(listed),
        onclick: () => {
          listed = !listed;
          drawSavedNav();
        },
      },
      el('span', { class: 'ic' }, icon(listed ? 'down' : 'right')),
      el('span', { class: 'grow' }, 'Views'),
      el('span', { class: 'meta' }, String(all.length)),
    ),
    el(
      'div',
      {
        class: 'nav-nest nav-saved-list',
        role: 'group',
        'aria-label': 'Saved views',
        hidden: !listed,
      },
      el(
        'div',
        { class: 'nav-filter', role: 'group', 'aria-label': 'Which saved views' },
        filterButton('Here', true),
        filterButton('All', false),
      ),
      groups.length === 0 && el('div', { class: 'meta nav-empty' }, 'None about here'),
      groups.map((group) => [
        el('div', { class: 'nav-group meta' }, group.label),
        nested(group.views).map((node) => navRow(node, open)),
      ]),
    ),
  );
}

/** A view's row in the sidebar, and the views listed under it behind a
 *  fold, so the ones saved to be reached from it do not crowd the group. */
function navRow(node: Nested, open: ReadonlySet<string>): HTMLElement[] {
  const v = node.view;
  const inner = count(node);
  const folded = !navOpen.has(v.id);
  const line = el(
    'div',
    { class: 'nav-saved' },
    draggable(
      el(
        'button',
        {
          class: `nav nav-sub${open.has(v.id) ? ' on' : ''}`,
          type: 'button',
          title: `${v.view.title} · ${v.scope.label}${v.stale ? ' · stale' : ''}`,
          onclick: () => call('open_saved_view', { id: v.id, from: null }),
        },
        el('span', { class: 'grow ttl' }, v.view.title),
        v.stale &&
          el('span', {
            class: 'vstale-dot',
            title: 'stale',
            role: 'img',
            'aria-label': 'stale',
          }),
      ),
      v.panel,
      v.view.title,
    ),
    inner > 0 &&
      el(
        'button',
        {
          class: 'nav-twist',
          type: 'button',
          'aria-expanded': String(!folded),
          title: `${folded ? 'Show' : 'Hide'} the ${inner} ${inner === 1 ? 'view' : 'views'} listed under it`,
          onclick: () => {
            if (folded) navOpen.add(v.id);
            else navOpen.delete(v.id);
            drawSavedNav();
          },
        },
        el('span', { class: 'meta' }, String(inner)),
        icon(folded ? 'right' : 'down'),
      ),
  );
  if (inner === 0 || folded) return [line];
  return [
    line,
    el(
      'div',
      { class: 'nav-nest', role: 'group', 'aria-label': `Linked from ${v.view.title}` },
      node.inner.map((n) => navRow(n, open)),
    ),
  ];
}

function filterButton(label: string, here: boolean): HTMLElement {
  return el(
    'button',
    {
      class: `chip${hereOnly === here ? ' on' : ''}`,
      type: 'button',
      'aria-pressed': String(hereOnly === here),
      onclick: () => {
        hereOnly = here;
        drawSavedNav();
      },
    },
    label,
  );
}

/** Whether a dock shows the panel now. */
function shownIn(panel: Panel): boolean {
  const docks = state.docks;
  return (
    !!docks &&
    ([docks.left, docks.right, docks.bottom].some((d) => d.shown === panel) ||
      docks.floating.some((f) => f.panel === panel))
  );
}
