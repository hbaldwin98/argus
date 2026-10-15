// The docks: the left, right and bottom edges of the stage a panel sits
// against, each showing one of its tabs, sized by its sash. Rust decides
// which dock holds each panel, which it shows and how big it is
// (`src/docks.rs`); this puts each panel's element where that says — its
// dock, the stage, or parked out of sight — draws a dock's tabs while it
// holds more than one, and reports a tab picked, closed, a panel moved or
// a sash let go.

import type { Dock } from './bindings/Dock';
import type { DockPage } from './bindings/DockPage';
import type { Panel } from './bindings/Panel';
import { $, el, icon, type IconName } from './dom.js';
import { draggable } from './drag.js';
import { dragEdge } from './edge.js';
import { extensionElement, extensionOnStage } from './extension_views.js';
import { savedElement } from './saved_views.js';
import { contextMenu, type MenuItem } from './menu.js';
import { closesOnMiddle } from './middle.js';
import { state } from './state.js';
import { call } from './tauri.js';

const DOCKS = ['left', 'right', 'bottom'] as const;

const WHERE: Record<Dock, string> = {
  left: 'on the left',
  right: 'on the right',
  bottom: 'at the bottom',
};

export const DOCK_ICON: Record<Dock, IconName> = {
  left: 'dockLeft',
  right: 'dockRight',
  bottom: 'dockBottom',
};

function dockPage(dock: Dock): DockPage | null {
  return state.docks?.[dock] ?? null;
}

/** The dock a panel is in, or `null` for a page or a panel no dock lists. */
export function dockOf(panel: Panel): Dock | null {
  return DOCKS.find((dock) => dockPage(dock)?.tabs.some((t) => t.panel === panel)) ?? null;
}

/** "Dock on the right", for a button or a menu. */
export function dockWords(dock: Dock): string {
  return `Dock ${WHERE[dock]}`;
}

/** A menu's items moving `panel` to each dock, the one it is shown in
 *  marked. */
export function moveItems(panel: Panel, docked: boolean): MenuItem[] {
  const own = dockOf(panel);
  return DOCKS.map((dock) => ({
    label: dockWords(dock),
    run: () => call('docks', { gesture: { on: 'move', panel, dock } }),
    disabled: docked && dock === own && 'here',
  }));
}

/** The panel's element on screen in a dock, or `null` while it is on the
 *  stage instead: the console as the Console stage or fullscreen, an
 *  extension's view as the page. */
function element(panel: Panel): HTMLElement | null {
  if (panel === 'console') return state.console?.place === 'docked' ? $('console') : null;
  if (panel === 'views') return $('views');
  if (panel.startsWith('saved:')) return savedElement(panel);
  return extensionOnStage(panel) ? null : extensionElement(panel);
}

/** Puts each dock's shown panel in it and parks the rest; the console on
 *  the stage goes to the stage. The keys' text area may be in a panel
 *  moved, and keeps its focus across the move. */
export function drawDocks(): void {
  const input = $('input');
  const hadKeys = document.activeElement === input;
  const kept = new Set<HTMLElement>();
  for (const dock of DOCKS) {
    const page = dockPage(dock);
    const shown = page?.shown ?? null;
    const panel = shown === null ? null : element(shown);
    const box = $(`dock-${dock}`);
    const body = $(`dock-${dock}-body`);
    // Parked rather than dropped, so a panel moved on keeps its state and
    // its ids stay in the document. A copy: parking empties the live list.
    for (const child of Array.from(body.children)) if (child !== panel) $('parked').append(child);
    if (panel) {
      kept.add(panel);
      if (panel.parentElement !== body) body.append(panel);
    }
    box.hidden = !panel;
    drawTabs(dock, page, panel ? shown : null);
    const size = page?.size;
    if (size) box.style[dock === 'bottom' ? 'height' : 'width'] = `${size}px`;
  }
  $('dock-bottom-sash').hidden = $('dock-bottom').hidden;
  const consoleEl = $('console');
  if (!kept.has(consoleEl)) {
    const onStage = state.console?.place === 'tab' || state.console?.place === 'fullscreen';
    const home = onStage ? $('center') : $('parked');
    if (consoleEl.parentElement !== home) home.append(consoleEl);
  }
  if (hadKeys && document.activeElement !== input) input.focus();
}

/** A dock's tabs, drawn only while it holds more than one panel. */
function drawTabs(dock: Dock, page: DockPage | null, shown: Panel | null): void {
  const strip = $(`dock-${dock}-tabs`);
  const tabs = page?.tabs ?? [];
  strip.hidden = shown === null || tabs.length < 2;
  if (strip.hidden) {
    strip.replaceChildren();
    return;
  }
  strip.replaceChildren(
    ...tabs.map((tab) => {
      const close = closeOf(tab.panel);
      const pick = draggable(
        el(
          'button',
          {
            class: 'dock-tab-pick',
            role: 'tab',
            type: 'button',
            title: tab.title,
            'aria-selected': String(tab.panel === shown),
            onclick: () => call('docks', { gesture: { on: 'show', panel: tab.panel } }),
          },
          el('span', { class: 'ttl' }, tab.title),
        ),
        tab.panel,
        tab.title,
      );
      const node = el(
        'div',
        {
          class: `dock-tab${tab.panel === shown ? ' on' : ''}`,
          oncontextmenu: contextMenu(tab.title, () => [
            { heading: tab.title },
            ...moveItems(tab.panel, true),
            ...(close ? (['sep', { label: 'Close', run: close }] as MenuItem[]) : []),
          ]),
        },
        pick,
        close &&
          el(
            'button',
            {
              class: 'icon x',
              type: 'button',
              title: 'Close',
              'aria-label': `Close ${tab.title}`,
              onclick: close,
            },
            icon('close'),
          ),
      );
      return close ? closesOnMiddle(node, close) : node;
    }),
  );
}

/** What closing a panel's tab does, or `null` for one with no close of its
 *  own: a saved view's panel goes, with its columns. The console, the
 *  agents' views and an extension's view are there while what they show
 *  is, and their dock's hide puts them away. */
function closeOf(panel: Panel): (() => void) | null {
  if (!panel.startsWith('saved:')) return null;
  return () => call('close_saved_column', { panel, column: 0 });
}

/** Each dock follows its sash while it is held, and its size is reported
 *  once it is let go, for Rust to keep and send back. */
export function listen(): void {
  // The console's head is drawn into the same element every time, so it
  // is made draggable once; a press on one of its tabs or buttons stays
  // theirs.
  draggable($('console-head'), 'console', 'Console');
  const work = $('work');
  const center = $('center');
  const size = (dock: Dock) => (at: number) => {
    if (dock === 'bottom') return center.getBoundingClientRect().bottom - at;
    const box = work.getBoundingClientRect();
    return dock === 'left' ? at - box.left : box.right - at;
  };
  for (const dock of DOCKS) {
    const box = $(`dock-${dock}`);
    dragEdge($(`dock-${dock}-sash`), box, {
      sizeAt: size(dock),
      vertical: dock === 'bottom',
      draw: (px) => {
        box.style[dock === 'bottom' ? 'height' : 'width'] = `${px}px`;
      },
      report: (px) => {
        // Drawn from what Rust keeps, which may hold it to its bounds and
        // so send nothing new; a double-click has no usual size to go to.
        drawDocks();
        if (px !== null) call('docks', { gesture: { on: 'size', dock, size: px } });
      },
    });
  }
}
