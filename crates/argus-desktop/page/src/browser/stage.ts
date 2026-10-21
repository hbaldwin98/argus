// The browsers agents' panes run, where they are listed rather than worked
// in: the Browsers stage, every browser at once as live tiles, and the
// Browsers fold in the sidebar. A browser's own pane — tabs, address bar
// and page — is a cell of the Workspace's grid (`pane.ts`); a click here
// puts it on screen there. Rust says which browsers there are and their
// tabs (`src/browser/mod.rs`); their pictures come apart (`shots.ts`).
// A tile says nothing to the daemon: it shows what the cell was sent,
// smaller.

import type { BrowsersPage } from '../bindings/BrowsersPage';
import type { PageBrowser } from '../bindings/PageBrowser';
import { $, el, icon } from '../kit/dom.js';
import { dragFromRail, drawSplit } from '../panes/split.js';
import { state } from '../state.js';
import { call } from '../connection/tauri.js';
import { forgetViews, updateBrowserViews } from './pane.js';
import { browsers, setBrowsers } from './page.js';
import { canvasFor, forgetShots, hearShot, paint, shotPanes } from './shots.js';
import { dotKind, status } from './status.js';
import type { BrowserShot } from '../bindings/BrowserShot';

/** Whether the open browsers are listed under Browsers in the sidebar. */
let listed = true;

export function browsersHeard(event: BrowsersPage): void {
  setBrowsers(event);
  const open = new Set(event.browsers.map((b) => b.pane));
  for (const pane of shotPanes()) if (!open.has(pane)) forgetShots(pane);
  forgetViews();
  drawBrowsersNav();
  drawBrowsers();
  updateBrowserViews();
  // A browser pane newly in the grid has its cell filled from this page.
  drawSplit();
}

export function browserShot(shot: BrowserShot): void {
  hearShot(shot);
}

/** The strip's title on this stage. */
export function browsersTitle(): string {
  return 'Every agent';
}

/** Whether `pane`'s browser is on screen in the Workspace's grid. */
function inGrid(pane: number): boolean {
  const { shown, row } = state.rail;
  return shown?.pane === pane || row.beside.some((s) => s?.pane === pane);
}

/** The open browsers under Browsers in the sidebar: a fold on its row, and
 *  a row each, named for its agent, which puts that browser's pane on
 *  screen and, dragged onto the Workspace, opens it there as a pane
 *  dragged from the rail would. */
export function drawBrowsersNav(): void {
  const count = browsers().length;
  const fold = $('nav-browsers-fold');
  const list = $('nav-browsers-list');
  $('nav-browsers-count').textContent = count > 0 ? String(count) : '';
  fold.hidden = count === 0;
  fold.setAttribute('aria-expanded', String(listed));
  fold.setAttribute('aria-label', listed ? 'Fold the open browsers' : 'List the open browsers');
  fold.replaceChildren(icon(listed ? 'down' : 'right'));
  list.hidden = count === 0 || !listed;
  list.replaceChildren(
    ...browsers().map((browser) => {
      const on = state.rail.stage === 'workspace' && inGrid(browser.pane);
      return el(
        'button',
        {
          class: `nav nav-sub${on ? ' on' : ''}`,
          type: 'button',
          title: `${browser.agent} · ${browser.url}`,
          'aria-current': on ? 'page' : null,
          onmousedown: (e: MouseEvent) => dragFromRail(e, browser.pane),
          onclick: () => call('show_browsers', { pane: browser.pane }),
        },
        el('span', { class: `ic bdot${dotKind(browser)}` }),
        el('span', { class: 'grow ttl' }, browser.agent),
        el('span', { class: 'meta ttl' }, browser.title || browser.url),
      );
    }),
  );
}

const foldToggle = (e: Event): void => {
  e.preventDefault();
  e.stopPropagation();
  listed = !listed;
  drawBrowsersNav();
};
$('nav-browsers-fold').addEventListener('click', foldToggle);

/** The Browsers stage: a tile for each browser, or why there are none. */
export function drawBrowsers(): void {
  const stage = $('bstage');
  if (stage.hidden) return;
  if (browsers().length === 0) stage.replaceChildren(empty());
  else stage.replaceChildren(el('div', { class: 'bgrid' }, browsers().map(tile)));
  for (const browser of browsers()) paint(browser.pane);
}

function empty(): HTMLElement {
  return el(
    'div',
    { class: 'empty' },
    el(
      'div',
      { class: 'empty-body' },
      el('p', null, 'No agent has a browser open.'),
      el(
        'p',
        { class: 'fg-muted' },
        'An agent opens one with ',
        el('code', null, 'argus-hook browser'),
        ' and drives it with its own browser tool; it shows here as it works.',
      ),
    ),
  );
}

/** Where the page is, said once: its title, and its address under it. */
function where(browser: PageBrowser): HTMLElement {
  const title = browser.title && browser.title !== browser.url ? browser.title : '';
  return el(
    'span',
    { class: 'bwhere' },
    title ? el('span', { class: 'btitle' }, title) : null,
    el('span', { class: 'burl mono fg-muted' }, browser.url || 'about:blank'),
  );
}

function tile(browser: PageBrowser): HTMLElement {
  return el(
    'button',
    {
      class: 'btile',
      type: 'button',
      title: `Open ${browser.agent}'s browser`,
      onclick: () => call('show_browsers', { pane: browser.pane }),
    },
    el(
      'span',
      { class: 'btile-head' },
      el(
        'span',
        { class: 'btile-agent' },
        el('span', { class: 'bagent' }, browser.agent),
        status(browser),
      ),
      where(browser),
    ),
    el('span', { class: 'btile-screen' }, canvasFor(browser.pane, 'tile')),
  );
}
