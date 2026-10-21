// The docks: the left, right and bottom edges of the stage a panel sits
// against, each showing one of its tabs, sized by its sash. Rust decides
// which dock holds each panel, which it shows and how big it is
// (`src/layout/docks.rs`); this puts each panel's element where that
// says — its dock, the stage, a dialog (`floats`), or parked out of
// sight — draws a dock's tabs while it holds more than one, and the dock
// menu every panel's head opens, and reports a tab picked, closed, a
// panel moved or a sash let go.

import type { Dock } from '../bindings/Dock';
import type { DockPage } from '../bindings/DockPage';
import type { Panel } from '../bindings/Panel';
import { $, el, icon, type IconName } from '../kit/dom.js';
import { draggable } from './drag.js';
import { dragEdge } from './edge.js';
import { extensionElement, extensionOnStage, extensionTools } from '../extensions/views.js';
import { drawFloats } from './floats.js';
import { savedElement } from '../views/saved.js';
import { contextMenu, type MenuItem, openMenu } from '../kit/menu.js';
import { closesOnMiddle } from '../kit/middle.js';
import { panelOf } from './panels.js';
import { state } from '../state.js';
import { call } from '../connection/tauri.js';

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

/** Whether the dock `panel` is in shows a tab strip, which then names it. */
export function dockShowsTabs(panel: Panel): boolean {
  const dock = dockOf(panel);
  return dock !== null && (dockPage(dock)?.tabs.length ?? 0) > 1;
}

/** "Dock on the right", for a button or a menu. */
export function dockWords(dock: Dock): string {
  return `Dock ${WHERE[dock]}`;
}

/** A menu's items moving `panel` to each dock, the one it is shown in
 *  marked, and for a panel that floats, floating it as a dialog. */
export function moveItems(panel: Panel, docked: boolean): MenuItem[] {
  const own = dockOf(panel);
  const items: MenuItem[] = DOCKS.map((dock) => ({
    label: dockWords(dock),
    run: () => call('docks', { gesture: { on: 'move', panel, dock } }),
    disabled: docked && dock === own && 'here',
  }));
  if (!panelOf(panel)?.floats) return items;
  const floating = state.docks?.floating.some((f) => f.panel === panel) ?? false;
  return [
    ...items,
    'sep',
    {
      label: 'Float as a dialog',
      run: () => call('docks', { gesture: { on: 'float', panel } }),
      disabled: floating && 'here',
    },
  ];
}

/** A panel head's "Move to another dock" button, opening the dock menu,
 *  titled `label`, under itself. Its icon is the dock the panel sits in
 *  while `docked`, and a chevron while it sits in none on screen. A head
 *  with a Dock button of its own, which already shows that dock, asks for
 *  the `chevron` always, so one icon never means two buttons there. */
export function dockMenuButton(
  panel: Panel,
  label: string,
  docked: boolean,
  look: 'dock' | 'chevron' = 'dock',
): HTMLElement {
  const own = docked && look === 'dock' ? dockOf(panel) : null;
  return el(
    'button',
    {
      class: 'icon',
      type: 'button',
      title: 'Move to another dock',
      'aria-label': 'Move to another dock',
      'aria-haspopup': 'menu',
      onclick: (e: MouseEvent) => {
        const box = (e.currentTarget as HTMLElement).getBoundingClientRect();
        openMenu(label, moveItems(panel, docked), { x: box.left, y: box.bottom + 4 });
      },
    },
    icon(own ? DOCK_ICON[own] : 'down'),
  );
}

/** The panel's element on screen in a dock, or `null` while it is on the
 *  stage instead: the console as the Console stage or fullscreen, an
 *  extension's view as the page. A panel Rust did not say exists has none. */
function element(panel: Panel): HTMLElement | null {
  switch (panelOf(panel)?.kind) {
    case 'console':
      return state.console?.place === 'docked' ? $('console') : null;
    case 'views':
      return $('views');
    case 'browser':
      return $('browser');
    case 'saved':
      return savedElement(panel);
    case 'extension':
      return extensionOnStage(panel) ? null : extensionElement(panel);
    case undefined:
      return null;
  }
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
  drawFloats(state.docks?.floating ?? [], element);
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
  const own = tabs.find((tab) => tab.panel === shown);
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
            'sep',
            close
              ? { label: 'Close', run: close }
              : {
                  label: 'Hide',
                  run: () => call('docks', { gesture: { on: 'hide', panel: tab.panel } }),
                },
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
    // The shown panel's dock menu rides the strip's end, not a row of its own.
    el('span', { class: 'grow' }),
    ...(shown ? extensionTools(shown) : []),
    own && shown ? dockMenuButton(shown, own.title, true) : '',
  );
}

/** What closing a panel's tab does, or `null` for a panel Rust says has no
 *  close of its own: its dock's hide puts it away. */
function closeOf(panel: Panel): (() => void) | null {
  if (!panelOf(panel)?.closes) return null;
  return () => call('docks', { gesture: { on: 'close', panel } });
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
