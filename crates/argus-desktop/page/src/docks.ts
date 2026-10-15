// The docks: the left, right and bottom edges of the stage a panel sits
// against, each showing one of its tabs, sized by its sash. Rust decides
// which dock holds each panel, which it shows and how big it is
// (`src/docks.rs`); this puts each panel's element where that says, sizes
// the docks, and reports a panel moved or a sash let go.

import type { Dock } from './bindings/Dock';
import type { Panel } from './bindings/Panel';
import { $, type IconName } from './dom.js';
import { dragEdge } from './edge.js';
import type { MenuItem } from './menu.js';
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

export function dockOf(panel: Panel): Dock {
  return DOCKS.find((dock) => state.docks?.[dock].tabs.includes(panel)) ?? 'bottom';
}

/** "Dock on the right", for a button or a menu. */
export function dockWords(dock: Dock): string {
  return `Dock ${WHERE[dock]}`;
}

/** A menu's items moving `panel` to each dock, its own marked. */
export function moveItems(panel: Panel, docked: boolean): MenuItem[] {
  const own = dockOf(panel);
  return DOCKS.map((dock) => ({
    label: dockWords(dock),
    run: () => call('docks', { gesture: { on: 'move', panel, dock } }),
    disabled: docked && dock === own && 'here',
  }));
}

/** Each panel's element, and whether it is in its dock now rather than
 *  on the stage: the Console stage and fullscreen take the console out. */
const PANELS: Record<Panel, { element: () => HTMLElement; docked: () => boolean }> = {
  console: { element: () => $('console'), docked: () => state.console?.place === 'docked' },
};

/** Puts each panel in its dock or on the stage, and shows the docks that
 *  hold one. The keys' text area may be in a panel moved, and keeps its
 *  focus across the move. */
export function drawDocks(): void {
  const filled = new Set<Dock>();
  const input = $('input');
  const hadKeys = document.activeElement === input;
  for (const [panel, { element, docked }] of Object.entries(PANELS) as [
    Panel,
    (typeof PANELS)[Panel],
  ][]) {
    const dock = docked() ? dockOf(panel) : null;
    const home = dock ? $(`dock-${dock}`) : $('center');
    if (element().parentElement !== home) home.append(element());
    if (dock) filled.add(dock);
  }
  if (hadKeys && document.activeElement !== input) input.focus();
  for (const dock of DOCKS) {
    const box = $(`dock-${dock}`);
    box.hidden = !filled.has(dock);
    const size = state.docks?.[dock].size;
    if (size) box.style[dock === 'bottom' ? 'height' : 'width'] = `${size}px`;
  }
  $('dock-bottom-sash').hidden = !filled.has('bottom');
}

/** Each dock follows its sash while it is held, and its size is reported
 *  once it is let go, for Rust to keep and send back. */
export function listen(): void {
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
