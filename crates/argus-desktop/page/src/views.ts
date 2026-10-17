// The panel the views agents showed open in — a panel `docks` puts in its
// dock, the right one until it is moved, beside whatever stage is up so the
// rail and the stage stay in sight — and the button in the title strip that
// brings it back. Rust holds the views, which one is chosen, and whether
// the panel is on screen; this draws them and says what was pressed.

import type { Link } from './bindings/Link';
import type { PageView } from './bindings/PageView';
import type { ViewsPage } from './bindings/ViewsPage';
import { actionButton } from './actions.js';
import { blocks } from './blocks.js';
import { chip } from './chip.js';
import { dockMenuButton } from './docks.js';
import { draggable } from './drag.js';
import { $, el, icon } from './kit/dom.js';
import { closesOnMiddle } from './kit/middle.js';
import { state } from './state.js';
import { call } from './connection/tauri.js';
import { ago } from './values.js';
import type { ViewCtx } from './viewctx.js';
import { viewLink } from './viewlink.js';

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
  launcher.hidden = viewsOut() || waiting === 0;
  launcher.replaceChildren(
    el('span', { class: 'vdot', 'aria-hidden': 'true' }),
    `${waiting} ${waiting === 1 ? 'view' : 'views'}`,
  );
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
    el('span', { class: 'grow' }),
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

function header(v: PageView, popped: boolean): HTMLElement {
  const at = new Date(Number(v.at) * 1000).toISOString();
  const subject = v.subject;
  return el(
    'div',
    { class: 'vpanel-head' },
    el(
      'div',
      { class: 'vpanel-title' },
      el('span', { class: 'type-heading' }, v.view.title),
      v.view.status && chip(v.view.status),
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
      el(
        'span',
        { class: 'vpanel-acts' },
        v.view.actions.map((a) => actionButton(a, ctxOf(v))),
        el(
          'button',
          {
            class: 'btn sm',
            type: 'button',
            disabled: v.refreshing,
            title: 'Ask the agent to read it again',
            onclick: () => call('refresh_view', { pane: v.pane, id: v.view.id }),
          },
          'Refresh',
        ),
        el(
          'button',
          {
            class: 'btn sm',
            type: 'button',
            title: popped ? 'Put the view back in its dock' : 'Open the view as a page',
            // Opening it in the dock is what ends the page.
            onclick: () =>
              call(popped ? 'open_view' : 'pop_out_view', { pane: v.pane, id: v.view.id }),
          },
          popped ? 'Dock' : 'Pop out',
        ),
        el(
          'button',
          {
            class: 'btn sm',
            type: 'button',
            title: 'Close this view for every window',
            onclick: () => call('close_view', { pane: v.pane, id: v.view.id }),
          },
          'Close',
        ),
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

/** Brings the newest view out, from the title strip's button. */
export function openNewestView(): void {
  const newest = shown.views[0];
  if (newest) call('open_view', { pane: newest.pane, id: newest.view.id });
}
