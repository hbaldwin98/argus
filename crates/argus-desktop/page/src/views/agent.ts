// The panel the views agents showed open in — a panel `docks` puts in its
// dock, the right one until it is moved, beside whatever stage is up so the
// rail and the stage stay in sight — and the button in the title strip that
// brings it back. Rust holds the views, which one is chosen, and whether
// the panel is on screen; this draws them and says what was pressed.

import type { Link } from '../bindings/Link';
import type { PageView } from '../bindings/PageView';
import type { Tone } from '../bindings/Tone';
import type { ViewsPage } from '../bindings/ViewsPage';
import { actionButton } from './actions.js';
import { blocks } from './blocks.js';
import { chip } from './chip.js';
import { dockMenuButton } from '../layout/docks.js';
import { draggable } from '../layout/drag.js';
import { $, el, icon } from '../kit/dom.js';
import { openMenu, type MenuItem } from '../kit/menu.js';
import { closesOnMiddle } from '../kit/middle.js';
import { state } from '../state.js';
import { call } from '../connection/tauri.js';
import { ago } from './values.js';
import type { ViewCtx } from './ctx.js';
import { viewLink } from './link.js';

let shown: ViewsPage = { views: [], open: null, popped: null, canPress: false };

/** Whether a dock shows the views now. */
function viewsOut(): boolean {
  const docks = state.docks;
  return !!docks && [docks.left, docks.right, docks.bottom].some((d) => d.shown === 'views');
}

/** Draws the panel from the agents' views `page`, or from the ones last
 *  sent when the docks changed, and the strip's button while the panel is
 *  not out. */
export function drawViews(page: ViewsPage = shown): void {
  shown = page;
  const panel = $('views-inner');
  const launcher = $('views-tab');
  const open =
    page.open && page.views.find((v) => v.pane === page.open?.pane && v.view.id === page.open?.id);
  const waiting = page.views.length;
  // Kept while the panel is out, lit, so it is the way to close a drawer.
  const out = viewsOut();
  launcher.hidden = waiting === 0;
  launcher.setAttribute('aria-pressed', String(out));
  launcher.replaceChildren('Views', el('span', { class: 'count' }, String(waiting)));
  launcher.title = `${waiting} ${waiting === 1 ? 'view' : 'views'} agents showed`;
  drawPopped(page);
  if (!open) {
    panel.replaceChildren();
    return;
  }
  // Keep the reader's place when the view comes again or another is added.
  const scroller = panel.querySelector('.vpanel-body');
  const top = scroller?.getAttribute('data-for') === key(open) ? scroller.scrollTop : 0;
  panel.replaceChildren(tabs(page.views, open), header(open, false), body(open));
  const fresh = panel.querySelector('.vpanel-body');
  if (fresh) fresh.scrollTop = top;
  // The open tab is never the one the strip scrolled away.
  panel.querySelector('.vpanel-tab.on')?.scrollIntoView({ block: 'nearest', inline: 'nearest' });
}

/** The view open as a page over the stage: the header and body the panel
 *  has, without its tabs. Rust says which one, and when there is none. */
function drawPopped(page: ViewsPage): void {
  const stage = $('vstage');
  const popped =
    page.popped &&
    page.views.find((v) => v.pane === page.popped?.pane && v.view.id === page.popped?.id);
  if (!popped) {
    stage.replaceChildren();
    return;
  }
  const scroller = stage.querySelector('.vpanel-body');
  const top = scroller?.getAttribute('data-for') === key(popped) ? scroller.scrollTop : 0;
  stage.replaceChildren(header(popped, true), body(popped));
  const fresh = stage.querySelector('.vpanel-body');
  if (fresh) fresh.scrollTop = top;
}

function key(v: PageView): string {
  return `${v.pane}/${v.view.id}`;
}

function tabs(views: readonly PageView[], open: PageView | null): HTMLElement {
  const strip = el(
    'div',
    { class: 'vpanel-tabs', role: 'tablist', 'aria-label': 'Views' },
    el(
      'div',
      { class: 'tabs-scroll' },
      views.map((v) => {
        const close = (): void => call('close_view', { pane: v.pane, id: v.view.id });
        return closesOnMiddle(
          el(
            'div',
            { class: `vpanel-tab${v === open ? ' on' : ''}` },
            el(
              'button',
              {
                class: 'vpanel-tab-pick',
                role: 'tab',
                type: 'button',
                'aria-selected': String(v === open),
                title: v.from ? `${v.view.title} · from ${v.from}` : v.view.title,
                onclick: () => call('open_view', { pane: v.pane, id: v.view.id }),
              },
              v.refreshing && el('span', { class: 'vspin', 'aria-label': 'refreshing' }),
              v.unseen && el('span', { class: 'vnew', role: 'img', 'aria-label': 'changed' }),
              el('span', { class: 'ttl' }, v.view.title),
            ),
            el(
              'button',
              {
                class: 'icon x',
                type: 'button',
                title: 'Close this view for every window',
                'aria-label': `Close ${v.view.title}`,
                onclick: close,
              },
              icon('close'),
            ),
          ),
          close,
        );
      }),
    ),
    dockMenuButton('views', 'Dock the views', true),
    el(
      'button',
      {
        class: 'icon',
        type: 'button',
        'aria-label': 'Hide the views',
        title: 'Hide',
        onclick: () => call('hide_views', {}),
      },
      icon('close'),
    ),
  );
  return draggable(strip, 'views', 'Views');
}

/** The view's actions and its overflow: in the panel they sit on the
 *  title row, as a page the stage's strip carries them. */
function viewActions(v: PageView, popped: boolean): HTMLElement[] {
  const act = {
    refresh: () => call('refresh_view', { pane: v.pane, id: v.view.id }),
    place: () => call(popped ? 'open_view' : 'pop_out_view', { pane: v.pane, id: v.view.id }),
    close: () => call('close_view', { pane: v.pane, id: v.view.id }),
  };
  const items = (): MenuItem[] => [
    { label: 'Refresh', disabled: v.refreshing && 'Already asked', run: act.refresh },
    // Opening it in the dock is what ends the page.
    { label: popped ? 'Put back in its dock' : 'Pop out as a page', run: act.place },
    'sep',
    { label: 'Close this view for every window', danger: true, run: act.close },
  ];
  const more = el(
    'button',
    {
      class: 'icon',
      type: 'button',
      title: 'More: refresh, pop out, close',
      'aria-label': `More about ${v.view.title}`,
      'aria-haspopup': 'menu',
      onclick: (e: MouseEvent) => {
        const box = (e.currentTarget as HTMLElement).getBoundingClientRect();
        openMenu(v.view.title, items(), { x: box.right, y: box.bottom + 4, align: 'end' });
      },
    },
    icon('more'),
  );
  return [...v.view.actions.map((a) => actionButton(a, ctxOf(v))), more];
}

function header(v: PageView, popped: boolean): HTMLElement {
  const at = new Date(Number(v.at) * 1000).toISOString();
  const subject = v.subject;
  return el(
    'div',
    { class: 'vpanel-head' },
    // One row: the name and its status at the start, the actions at the end.
    // As a page the stage's strip carries all three, so only the meta is left.
    !popped &&
      el(
        'div',
        { class: 'vpanel-title' },
        el('span', { class: 'type-heading vpanel-name' }, v.view.title),
        v.view.status && chip(v.view.status),
        el('span', { class: 'vpanel-acts' }, viewActions(v, false)),
      ),
    el(
      'div',
      { class: 'vpanel-meta' },
      subject &&
        viewLink(
          subject.url,
          el('span', { class: 'track' }, subject.tracker, ' ', el('b', null, subject.key)),
        ),
      el(
        'span',
        { class: 'meta', title: new Date(at).toLocaleString() },
        v.from ? `read by ${v.from} · ${ago(at)}` : ago(at),
        v.refreshing ? ' · asked to read it again' : '',
      ),
    ),
  );
}

/** What the view's blocks, and the buttons in its header, are drawn with. */
function ctxOf(v: PageView): ViewCtx {
  return {
    kind: 'agent',
    pane: v.pane,
    id: v.view.id,
    path: key(v),
    drawn: (source: string) => v.diagrams.find((d) => d.source === source),
    pressed: v.pressed,
    canPress: shown.canPress,
    changed: new Set(v.changed),
  };
}

function body(v: PageView): HTMLElement {
  return el('div', { class: 'vpanel-body', 'data-for': key(v) }, blocks(v.view.blocks, ctxOf(v)));
}

/** The newest view of a ticket, matched by its URL or its tracker and key. */
export function viewOf(link: Link): PageView | undefined {
  return shown.views.find(
    (v) =>
      v.subject !== null &&
      (v.subject.url === link.url ||
        (v.subject.tracker === link.tracker && v.subject.key === link.key)),
  );
}

/** The title of the view open as a page, for the strip. */
export function poppedViewTitle(): string | null {
  const at = shown.popped;
  return at
    ? (shown.views.find((v) => v.pane === at.pane && v.view.id === at.id)?.view.title ?? null)
    : null;
}

/** What the stage's strip carries for the view open as a page: its status
 *  beside the title, and its actions at the right. */
export function poppedViewHead(): { status: HTMLElement | null; tools: HTMLElement[] } | null {
  const at = shown.popped;
  const v = at && shown.views.find((p) => p.pane === at.pane && p.view.id === at.id);
  if (!v) return null;
  const status = v.view.status;
  return {
    status: status && el('span', { class: `spill ${SPILL[status.tone]}` }, status.label),
    tools: viewActions(v, true),
  };
}

/** The strip's one pill speaks the statuses; a view's tone picks among them. */
const SPILL: Record<Tone, string> = {
  neutral: 'idle',
  good: 'done',
  warn: 'waiting',
  bad: 'failed',
  active: 'working',
  info: 'info',
};

/** Brings the newest view out, from the title strip's button, which hides
 *  the panel instead while it is out. */
export function openNewestView(): void {
  if (viewsOut()) {
    call('hide_views', {});
    return;
  }
  const newest = shown.views[0];
  if (newest) call('open_view', { pane: newest.pane, id: newest.view.id });
}
