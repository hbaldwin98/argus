// A browser pane's cell in the Workspace grid: the tabs, the address bar and
// the page, drawn where a terminal would be. The view of each browser is
// made once and kept, so the page's focus and the address being typed
// survive the redraws that follow every change of title or tab; only the
// tab strip is rebuilt.
//
// A browser drawn here tells the daemon the room it has and the screen's
// pixel density, and the page is laid out at that size and pictured at
// that density, so it is drawn one to one rather than stretched.

import type { BrowserInput } from '../bindings/BrowserInput';
import type { BrowserTab } from '../bindings/BrowserTab';
import type { PageBrowser } from '../bindings/PageBrowser';
import { el, icon } from '../kit/dom.js';
import { closesOnMiddle } from '../kit/middle.js';
import { actionFor } from '../keys/keymap.js';
import { state } from '../state.js';
import { call } from '../connection/tauri.js';
import { address, keyInput } from './input.js';
import { browsers, byPane } from './page.js';
import { canvasFor, paint, point } from './shots.js';
import { status } from './status.js';

interface View {
  root: HTMLElement;
  strip: HTMLElement;
  canvas: HTMLCanvasElement;
  field: HTMLInputElement;
  observer: ResizeObserver;
}

const views = new Map<number, View>();
/** The viewport each browser was last told, so a redraw says it again
 *  only when it changed. */
const told = new Map<number, string>();

/** Sends the person's hand to `pane`'s browser. */
function send(pane: number, input: BrowserInput): void {
  call('browser_input', { pane, input });
}

/** Gives the keys to `pane`'s cell, as a press on a terminal does, so the
 *  pane commands Rust's keymap runs are aimed at it. */
function keyPane(pane: number): void {
  const { shown, row } = state.rail;
  const beside = row.beside.findIndex((s) => s?.pane === pane);
  const slot = shown?.pane === pane ? 0 : beside < 0 ? -1 : beside + 1;
  if (slot < 0 || row.keyed === slot) return;
  call(slot === 0 ? 'select' : 'focus_view', { pane });
}

/** The view of browser pane `pane`, made on first use. */
export function browserView(pane: number): HTMLElement {
  const held = views.get(pane);
  if (held) return held.root;
  const canvas = canvasFor(pane, 'cell', (c) => {
    c.tabIndex = 0;
    c.addEventListener('mousedown', (e) => {
      c.focus();
      const at = point(pane, c, e);
      if (at) send(pane, { kind: 'click', ...at });
    });
    c.addEventListener('wheel', (e) => {
      e.preventDefault();
      const at = point(pane, c, e);
      if (at) send(pane, { kind: 'wheel', ...at, dx: e.deltaX, dy: e.deltaY });
    });
    c.addEventListener('keydown', (e) => {
      const input = keyInput(e);
      if (!input) return;
      // The browser's, not the window's: a digit typed into a form is not
      // a stage key.
      e.preventDefault();
      e.stopPropagation();
      send(pane, input);
    });
  });
  const field = urlField(pane);
  const strip = el('div', { class: 'vpanel-tabs btabs', role: 'tablist', 'aria-label': 'Tabs' });
  // No padding: the box is the viewport the page is laid out at, so the
  // canvas filling it draws the picture one pixel to one.
  const screen = el('div', { class: 'bpane-screen' }, canvas);
  const root = el(
    'div',
    { class: 'bpane' },
    strip,
    el(
      'div',
      { class: 'bbar' },
      barButton('←', 'Back', () => send(pane, { kind: 'back' })),
      barButton('→', 'Forward', () => send(pane, { kind: 'forward' })),
      barButton('⟳', 'Reload', () => send(pane, { kind: 'reload' })),
      field,
      barButton('Close', 'Close this browser; the agent’s next request opens another', () =>
        call('close_browser', { pane }),
      ),
    ),
    screen,
  );
  root.addEventListener('mousedown', () => keyPane(pane), true);
  const observer = watchSize(pane, screen);
  const view = { root, strip, canvas, field, observer };
  views.set(pane, view);
  update(pane, view);
  paint(pane);
  return root;
}

/** Redraws every browser view from what Rust last said. */
export function updateBrowserViews(): void {
  for (const [pane, view] of views) update(pane, view);
}

/** Lets go of the views of browsers that are no longer open. */
export function forgetViews(): void {
  const open = new Set(browsers().map((b) => b.pane));
  for (const [pane, view] of views) {
    if (open.has(pane)) continue;
    view.observer.disconnect();
    views.delete(pane);
    told.delete(pane);
  }
}

function update(pane: number, view: View): void {
  const browser = byPane(pane);
  view.strip.replaceChildren(...(browser ? tabControls(browser) : []));
  view.canvas.setAttribute('aria-label', browser ? `${browser.agent}'s browser` : 'Browser');
  if (browser && document.activeElement !== view.field) view.field.value = browser.url;
}

function tabName(tab: BrowserTab): string {
  return tab.title || tab.url || 'New tab';
}

/** The browser's own tabs — the one shown lit, each closed by its ×, and a
 *  + for a new one — then who has it, and the person's way to take it or
 *  give it back. */
function tabControls(browser: PageBrowser): HTMLElement[] {
  const pane = browser.pane;
  const controls = [
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
      : barButton('Follow agent', 'Show the tab the agent works in, as it moves', () =>
          send(pane, { kind: 'follow_agent' }),
        ),
    browser.held
      ? barButton('Hand back', 'Let the agent drive again', () =>
          send(pane, { kind: 'hold', held: false }),
        )
      : barButton(
          'Take control',
          'Hold the browser: the agent’s clicks and typing are refused until you hand it back',
          () => send(pane, { kind: 'hold', held: true }),
        ),
  ];
  return controls.filter((c): c is HTMLElement => c !== null);
}

/** Tells the daemon the room `screen` has whenever it changes, once it is
 *  laid out: a box not on screen has none, and says nothing. */
function watchSize(pane: number, screen: HTMLElement): ResizeObserver {
  const observer = new ResizeObserver(() => {
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
  return observer;
}

function barButton(label: string, title: string, onclick: () => void): HTMLElement {
  return el('button', { class: 'btn', type: 'button', title, onclick }, label);
}

/** The address field, which shows its browser's address unless the person
 *  is mid-edit. */
function urlField(pane: number): HTMLInputElement {
  const field = el('input', {
    class: 'field burl-field mono',
    'aria-label': 'Address',
    spellcheck: 'false',
  }) as HTMLInputElement;
  field.addEventListener('keydown', (e) => {
    // Its keys are its own, never the window's.
    e.stopPropagation();
    const now = byPane(pane);
    const action = actionFor(['address'], e);
    if (!now || !action) return;
    e.preventDefault();
    if (action === 'choose') send(pane, { kind: 'navigate', url: address(field.value) });
    else field.value = now.url;
    field.blur();
  });
  return field;
}
