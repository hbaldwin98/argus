// A checkout's file tree as one component, drawn by the Code stage and the
// Checkouts files panel alike so a checkout looks the same in both: a
// chevron, a folder or file icon, the name, its change mark, and the ignored
// top-level folders held in one row at the foot. Each surface keeps what is
// its own — what a click asks, the keys, the actions on a row — and hands it
// in; Rust has already ordered the rows and said which are open.

import { el, icon } from './dom.js';
import { foldIgnored, foldLabel, type Foldable } from './ignored_fold.js';

export interface TreeMark {
  /** `M`, `?`, or a count on a folder. */
  text: string;
  tone: 'changed' | 'untracked';
  title: string;
}

export interface TreeEntry extends Foldable {
  path: string;
  dir: boolean;
  open: boolean;
  on: boolean;
  mark: TreeMark | null;
}

export interface RowWays {
  /** A button where the surface tabs between rows; a div where the tree
   *  holds the focus itself. */
  tag: 'div' | 'button';
  /** Attributes of the row itself: its id, its tree roles. */
  attrs?: Record<string, unknown>;
  onclick: () => void;
  ondblclick?: () => void;
  oncontextmenu?: (e: MouseEvent) => void;
  /** Controls between the name and the mark, shown on the selected row. */
  extra?: HTMLElement | null;
}

function mark(m: TreeMark): HTMLElement {
  return el('span', { class: `cf-mark ${m.tone}`, title: m.title, 'aria-label': m.title }, m.text);
}

/** One row of the tree. */
export function fileRow(e: TreeEntry, ways: RowWays): HTMLElement {
  return el(
    ways.tag,
    {
      class: `cf-row${e.on ? ' on' : ''}${e.ignored ? ' ignored' : ''}${e.dir ? ' dir' : ''}`,
      title: e.path,
      style: `--d: ${e.depth}`,
      type: ways.tag === 'button' ? 'button' : null,
      ...ways.attrs,
      onclick: ways.onclick,
      ondblclick: ways.ondblclick,
      oncontextmenu: ways.oncontextmenu,
    },
    el('span', { class: `cf-chev${e.open ? ' open' : ''}` }, e.dir ? icon('right') : null),
    el('span', { class: 'cf-glyph' }, icon(e.dir ? 'folder-sm' : 'file-sm')),
    el('span', { class: 'cf-name' }, e.name),
    ways.extra,
    e.mark && mark(e.mark),
  );
}

/** Whether each surface's ignored row is open. Kept here, not in the DOM:
 *  every redraw builds the tree anew. */
const unfolded = new Set<string>();

/** The tree's rows, `make` drawing each, with the top-level ignored entries
 *  held in one row at the foot that opens to show them dimmed. `surface`
 *  names whose row it is, since the two trees open theirs on their own. */
export function treeRows<T extends Foldable>(
  items: readonly T[],
  surface: string,
  make: (item: T) => HTMLElement,
): HTMLElement[] {
  const { shown, held, names } = foldIgnored(items);
  const rows = shown.map(make);
  if (held.length === 0) return rows;
  const group = el('div', { class: 'ft-held' }, held.map(make));
  const row = el(
    'button',
    {
      class: 'ft-fold',
      type: 'button',
      'aria-expanded': String(unfolded.has(surface)),
      title: 'Ignored by git',
      onclick: () => {
        const open = !unfolded.has(surface);
        if (open) unfolded.add(surface);
        else unfolded.delete(surface);
        row.setAttribute('aria-expanded', String(open));
        group.hidden = !open;
      },
    },
    el('span', { class: 'cf-chev' }, icon('right')),
    foldLabel(names),
  );
  group.hidden = !unfolded.has(surface);
  return [...rows, row, group];
}
