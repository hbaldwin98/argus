// The browsers agents' panes run, in the two places the window shows them:
// the browser panel in its dock — one browser, the right dock's until it is
// moved — and the Browsers stage, every browser at once as live tiles or
// the chosen one filling it. Rust says which browsers there are, their
// tabs, which is chosen and which is docked (`src/browser/mod.rs`); their
// pictures come apart, many a second, and are painted onto each canvas
// showing that browser as they come. The page's CSP allows no `data:`
// image, so a picture is decoded to a bitmap and drawn rather than set as
// a source.
//
// A browser drawn large tells the daemon the room it has and the screen's
// pixel density, and the page is laid out at that size and pictured at
// that density, so it is drawn one to one rather than stretched. A tile
// says nothing: it shows what the large view was sent, smaller.

import type { BrowserInput } from '../bindings/BrowserInput';
import type { BrowserShot } from '../bindings/BrowserShot';
import type { BrowsersPage } from '../bindings/BrowsersPage';
import type { BrowserTab } from '../bindings/BrowserTab';
import type { PageBrowser } from '../bindings/PageBrowser';
import { $, el, icon } from '../kit/dom.js';
import { closesOnMiddle } from '../kit/middle.js';
import { actionFor } from '../keys/keymap.js';
import { dockMenuButton } from '../layout/docks.js';
import { draggable, type Target } from '../layout/drag.js';
import { state } from '../state.js';
import { call } from '../connection/tauri.js';
import { address, keyInput, within } from './input.js';

/** Where a browser is drawn: a tile on the stage, the stage's chosen
 *  browser, or the dock's. Each has a canvas of its own. */
type Place = 'tile' | 'stage' | 'dock';
const PLACES: readonly Place[] = ['tile', 'stage', 'dock'];

let page: BrowsersPage = { browsers: [], chosen: null, docked: null };
const shots = new Map<number, BrowserShot>();
const canvases = new Map<string, HTMLCanvasElement>();
/** The newest picture each browser has started decoding, so one that
 *  finishes after a newer one is dropped. */
const painting = new Map<number, number>();
/** The viewport each browser was last told, so a redraw says it again
 *  only when it changed. */
const told = new Map<number, string>();
const urlFields = new Map<Place, HTMLInputElement>();

/** Whether the open browsers are listed under Browsers in the sidebar. */
let listed = true;

export function browsersHeard(event: BrowsersPage): void {
  page = event;
  drawBrowsersNav();
  const open = new Set(event.browsers.map((b) => b.pane));
  for (const pane of shots.keys()) {
    if (!open.has(pane)) {
      shots.delete(pane);
      told.delete(pane);
      for (const place of PLACES) canvases.delete(`${pane}:${place}`);
    }
  }
  drawBrowsers();
  drawBrowserPanel();
}

export function browserShot(shot: BrowserShot): void {
  shots.set(shot.pane, shot);
  paint(shot.pane);
}

/** The strip's title on this stage: the chosen browser's agent. */
export function browsersTitle(): string {
  return byPane(page.chosen)?.agent ?? 'Every agent';
}

/** `pane`'s browser dropped on a dock, shown in the panel there, or on the
 *  stage, filling it. */
function placeBrowser(pane: number, target: Target): void {
  call('place_browser', { pane, dock: target === 'stage' ? null : target });
}

/** Makes `handle` drag `browser` to a dock or the stage. */
function dragsBrowser<T extends HTMLElement>(handle: T, browser: PageBrowser): T {
  return draggable(handle, 'browser', `${browser.agent}’s browser`, (target) =>
    placeBrowser(browser.pane, target),
  );
}

/** The open browsers under Browsers in the sidebar: a fold on its row, and
 *  a row each, which opens it and drags it to a dock. */
export function drawBrowsersNav(): void {
  const count = page.browsers.length;
  const fold = $('nav-browsers-fold');
  const list = $('nav-browsers-list');
  $('nav-browsers-count').textContent = count > 0 ? String(count) : '';
  fold.hidden = count === 0;
  fold.setAttribute('aria-expanded', String(listed));
  fold.setAttribute('aria-label', listed ? 'Fold the open browsers' : 'List the open browsers');
  fold.replaceChildren(icon(listed ? 'down' : 'right'));
  list.hidden = count === 0 || !listed;
  const docked = state.docks
    ? [state.docks.left, state.docks.right, state.docks.bottom].some((d) => d.shown === 'browser')
    : false;
  list.replaceChildren(
    ...page.browsers.map((browser) => {
      const on =
        (state.rail.stage === 'browsers' && page.chosen === browser.pane) ||
        (docked && page.docked === browser.pane);
      return dragsBrowser(
        el(
          'button',
          {
            class: `nav nav-sub${on ? ' on' : ''}`,
            type: 'button',
            title: `${browser.agent} · ${browser.url}`,
            'aria-current': on ? 'page' : null,
            onclick: () => call('show_browsers', { pane: browser.pane }),
          },
          el('span', { class: `ic bdot${dotKind(browser)}` }),
          el('span', { class: 'grow ttl' }, browser.agent),
          el('span', { class: 'meta ttl' }, browser.title || browser.url),
        ),
        browser,
      );
    }),
  );
}

function dotKind(browser: PageBrowser): string {
  if (browser.held) return ' held';
  if (browser.driver === 'agent') return ' agent';
  if (browser.driver === 'person') return ' person';
  return '';
}

const foldToggle = (e: Event): void => {
  e.preventDefault();
  e.stopPropagation();
  listed = !listed;
  drawBrowsersNav();
};
$('nav-browsers-fold').addEventListener('click', foldToggle);

function byPane(pane: number | null): PageBrowser | null {
  return page.browsers.find((b) => b.pane === pane) ?? null;
}

/** Whether the Browsers stage is up on `pane`'s browser, which the dock
 *  then leaves to it: one browser drawn large in two sizes would have the
 *  page laid out for each in turn. */
function onStage(pane: number): boolean {
  return state.rail.stage === 'browsers' && page.chosen === pane;
}

export function drawBrowsers(): void {
  const stage = $('bstage');
  if (stage.hidden) return;
  const chosen = byPane(page.chosen);
  if (chosen) {
    stage.replaceChildren(
      browserView(chosen, 'stage', [
        button('All browsers', 'Every agent’s browser at once', () =>
          call('show_browsers', { pane: null }),
        ),
      ]),
    );
  } else if (page.browsers.length === 0) {
    stage.replaceChildren(empty());
  } else {
    stage.replaceChildren(el('div', { class: 'bgrid' }, page.browsers.map(tile)));
  }
  for (const browser of page.browsers) paint(browser.pane);
}

/** The browser panel's body: the docked browser, under a header that drags
 *  the panel and docks or hides it. */
export function drawBrowserPanel(): void {
  const panel = $('browser-inner');
  const docked = byPane(page.docked);
  const strip = draggable(
    el(
      'div',
      { class: 'vpanel-head bpanel-head' },
      el('span', { class: 'bagent' }, docked ? docked.agent : 'Browser'),
      el('span', { class: 'grow' }),
      dockMenuButton('browser', 'Dock the browser', true),
      el(
        'button',
        {
          class: 'icon',
          type: 'button',
          'aria-label': 'Hide the browser',
          title: 'Hide',
          onclick: () => call('docks', { gesture: { on: 'hide', panel: 'browser' } }),
        },
        icon('close'),
      ),
    ),
    'browser',
    'Browser',
  );
  if (!docked) {
    panel.replaceChildren(strip, empty());
  } else if (onStage(docked.pane)) {
    panel.replaceChildren(
      strip,
      el(
        'div',
        { class: 'empty' },
        el(
          'div',
          { class: 'empty-body' },
          el('p', null, `${docked.agent}’s browser is open on the Browsers stage.`),
        ),
      ),
    );
  } else {
    panel.replaceChildren(
      strip,
      browserView(docked, 'dock', [
        button('Large', 'Open on the Browsers stage', () =>
          call('show_browsers', { pane: docked.pane }),
        ),
      ]),
    );
    paint(docked.pane);
  }
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

/** Sends the person's hand to `pane`'s browser. */
function send(pane: number, input: BrowserInput): void {
  call('browser_input', { pane, input });
}

/** A browser's canvas in one place, made once. Drawn large it answers the
 *  mouse and keys; as a tile, a press opens it instead. */
function canvasFor(pane: number, place: Place): HTMLCanvasElement {
  const held = canvases.get(`${pane}:${place}`);
  if (held) return held;
  const canvas = el('canvas', { class: 'bscreen' }) as HTMLCanvasElement;
  if (place !== 'tile') {
    canvas.tabIndex = 0;
    canvas.addEventListener('mousedown', (e) => {
      canvas.focus();
      const at = point(pane, canvas, e);
      if (at) send(pane, { kind: 'click', ...at });
    });
    canvas.addEventListener('wheel', (e) => {
      e.preventDefault();
      const at = point(pane, canvas, e);
      if (at) send(pane, { kind: 'wheel', ...at, dx: e.deltaX, dy: e.deltaY });
    });
    canvas.addEventListener('keydown', (e) => {
      const input = keyInput(e);
      if (!input) return;
      // The browser's, not the window's: a digit typed into a form is not
      // a stage key.
      e.preventDefault();
      e.stopPropagation();
      send(pane, input);
    });
  }
  canvases.set(`${pane}:${place}`, canvas);
  return canvas;
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
  const head = el(
    'span',
    { class: 'btile-head' },
    el(
      'span',
      { class: 'btile-agent' },
      el('span', { class: 'bagent' }, browser.agent),
      status(browser),
    ),
    where(browser),
  );
  const made = el(
    'button',
    {
      class: 'btile',
      type: 'button',
      title: `Open ${browser.agent}'s browser`,
      onclick: () => call('show_browsers', { pane: browser.pane }),
    },
    head,
    el('span', { class: 'btile-screen' }, canvasFor(browser.pane, 'tile')),
  );
  // The whole tile is the handle: dragged, it goes to a dock; pressed, it
  // opens on the stage.
  return dragsBrowser(made, browser);
}

/** Who has the browser, said only when someone does. */
function status(browser: PageBrowser): HTMLElement | null {
  if (browser.held) return el('span', { class: 'bstatus held' }, 'You have control');
  if (browser.driver === 'agent') return el('span', { class: 'bstatus agent' }, 'Agent driving');
  if (browser.driver === 'person') return el('span', { class: 'bstatus person' }, 'You’re driving');
  return null;
}

/** A browser drawn large: its tabs, its address bar with `lead` before
 *  it, and the page, sized to the room it is given. */
function browserView(browser: PageBrowser, place: Place, lead: HTMLElement[]): HTMLElement {
  const pane = browser.pane;
  const canvas = canvasFor(pane, place);
  canvas.setAttribute('aria-label', `${browser.agent}'s browser`);
  const screen = el('div', { class: 'bchosen-screen' }, canvas);
  watchSize(pane, screen);
  return el(
    'div',
    { class: 'bchosen' },
    tabStrip(browser, place),
    el(
      'div',
      { class: 'bbar' },
      ...lead,
      button('←', 'Back', () => send(pane, { kind: 'back' })),
      button('→', 'Forward', () => send(pane, { kind: 'forward' })),
      button('⟳', 'Reload', () => send(pane, { kind: 'reload' })),
      urlFieldFor(place, browser),
      button('Close', 'Close this browser; the agent’s next request opens another', () =>
        call('close_browser', { pane }),
      ),
    ),
    screen,
  );
}

function tabName(tab: BrowserTab): string {
  return tab.title || tab.url || 'New tab';
}

/** The browser's own tabs — the one shown lit, each closed by its ×, and a
 *  + for a new one — then who has it, and the person's way to take it or
 *  give it back. On the stage a grip drags it to a dock; in the panel, the
 *  panel's header does. */
function tabStrip(browser: PageBrowser, place: Place): HTMLElement {
  const pane = browser.pane;
  const grip =
    place === 'stage'
      ? dragsBrowser(
          el(
            'span',
            { class: 'bgrip', title: 'Drag to a dock', 'aria-label': 'Drag to a dock' },
            '⠿',
          ),
          browser,
        )
      : null;
  return el(
    'div',
    { class: 'vpanel-tabs btabs', role: 'tablist', 'aria-label': 'Tabs' },
    grip,
    el(
      'div',
      { class: 'tabs-scroll' },
      browser.tabs.map((tab) => {
        const shown = tab.id === browser.shown;
        const close = (): void => send(pane, { kind: 'close_tab', id: tab.id });
        return closesOnMiddle(
          el(
            'div',
            { class: `vpanel-tab${shown ? ' on' : ''}` },
            el(
              'button',
              {
                class: 'vpanel-tab-pick',
                role: 'tab',
                type: 'button',
                'aria-selected': String(shown),
                title: tab.url,
                onclick: () => send(pane, { kind: 'show_tab', id: tab.id }),
              },
              el('span', { class: 'ttl' }, tabName(tab)),
            ),
            el(
              'button',
              {
                class: 'icon x',
                type: 'button',
                title: 'Close this tab',
                'aria-label': `Close ${tabName(tab)}`,
                onclick: close,
              },
              icon('close'),
            ),
          ),
          close,
        );
      }),
    ),
    el(
      'button',
      {
        class: 'icon',
        type: 'button',
        title: 'New tab',
        'aria-label': 'New tab',
        onclick: () => send(pane, { kind: 'new_tab' }),
      },
      icon('plus'),
    ),
    status(browser),
    browser.following
      ? null
      : button('Follow agent', 'Show the tab the agent works in, as it moves', () =>
          send(pane, { kind: 'follow_agent' }),
        ),
    browser.held
      ? button('Hand back', 'Let the agent drive again', () =>
          send(pane, { kind: 'hold', held: false }),
        )
      : button(
          'Take control',
          'Hold the browser: the agent’s clicks and typing are refused until you hand it back',
          () => send(pane, { kind: 'hold', held: true }),
        ),
  );
}

/** Tells the daemon the room `screen` has whenever it changes, once it is
 *  laid out: a box not on screen has none, and says nothing. */
function watchSize(pane: number, screen: HTMLElement): void {
  const observer = new ResizeObserver(() => {
    if (!screen.isConnected) return observer.disconnect();
    const box = screen.getBoundingClientRect();
    const width = Math.floor(box.width);
    const height = Math.floor(box.height);
    if (width < 50 || height < 50) return;
    const scale = window.devicePixelRatio || 1;
    const viewport = `${width}x${height}@${scale}`;
    if (told.get(pane) === viewport) return;
    told.set(pane, viewport);
    send(pane, { kind: 'viewport', width, height, scale });
  });
  observer.observe(screen);
}

function button(label: string, title: string, onclick: () => void): HTMLElement {
  return el('button', { class: 'btn', type: 'button', title, onclick }, label);
}

/** A place's address field, made once so typing in it survives the page
 *  changing under it; it shows its browser's address unless the person is
 *  mid-edit. */
function urlFieldFor(place: Place, browser: PageBrowser): HTMLInputElement {
  let field = urlFields.get(place);
  if (!field) {
    const made = el('input', {
      class: 'field burl-field mono',
      'aria-label': 'Address',
      spellcheck: 'false',
    }) as HTMLInputElement;
    made.addEventListener('keydown', (e) => {
      // Its keys are its own, never the window's.
      e.stopPropagation();
      const now = byPane(Number(made.dataset.pane));
      const action = actionFor(['address'], e);
      if (!now || !action) return;
      e.preventDefault();
      if (action === 'choose') send(now.pane, { kind: 'navigate', url: address(made.value) });
      else made.value = now.url;
      made.blur();
    });
    urlFields.set(place, made);
    field = made;
  }
  field.dataset.pane = String(browser.pane);
  if (document.activeElement !== field) field.value = browser.url;
  return field;
}

/** The point under the mouse in the page's CSS pixels, which the browser
 *  takes its input in. */
function point(
  pane: number,
  canvas: HTMLCanvasElement,
  e: MouseEvent,
): { x: number; y: number } | null {
  const shot = shots.get(pane);
  const box = canvas.getBoundingClientRect();
  if (!shot || box.width === 0 || box.height === 0) return null;
  return within(box, shot, e.clientX, e.clientY);
}

/** Paints `pane`'s latest picture on every canvas on screen showing it. */
function paint(pane: number): void {
  const shot = shots.get(pane);
  const showing = PLACES.map((place) => canvases.get(`${pane}:${place}`)).filter(
    (canvas): canvas is HTMLCanvasElement => !!canvas?.isConnected,
  );
  if (!shot || showing.length === 0) return;
  const ticket = (painting.get(pane) ?? 0) + 1;
  painting.set(pane, ticket);
  const bytes = Uint8Array.from(atob(shot.jpeg), (c) => c.charCodeAt(0));
  createImageBitmap(new Blob([bytes], { type: 'image/jpeg' }))
    .then((bitmap) => {
      if (painting.get(pane) !== ticket) return bitmap.close();
      for (const canvas of showing) {
        if (canvas.width !== bitmap.width) canvas.width = bitmap.width;
        if (canvas.height !== bitmap.height) canvas.height = bitmap.height;
        canvas.getContext('2d')?.drawImage(bitmap, 0, 0);
      }
      bitmap.close();
    })
    .catch(() => {});
}
