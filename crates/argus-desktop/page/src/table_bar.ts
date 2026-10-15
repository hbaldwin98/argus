// The bar over an agent's table: the filter box, how many rows it lets
// through, how many are selected, the agent's actions on the selection, and
// Copy as TSV. An extension's table has none of it.

import type { Value } from './bindings/Value';
import { actionButton } from './actions.js';
import { cellText } from './cell_text.js';
import { copyButton } from './copy.js';
import { el } from './dom.js';
import type { TableBlock } from './table.js';
import { filterOf, setFilter } from './table_memory.js';
import type { ViewCtx } from './viewctx.js';

/** Past this many rows a table offers a filter. */
const FILTERABLE = 8;

/** What the bar needs of a row. */
export interface BarRow {
  tr: HTMLElement;
  /** An extension's selected-row buttons, a row of their own under it. */
  actions: HTMLElement | null;
  detail: HTMLElement | null;
  cells: readonly Value[];
  text: string;
  key: string | null;
  /** Whether the row is among those the filter lets through. */
  shown: boolean;
}

/** A row's cells as the text a filter matches and a copy writes. */
export function rowText(cells: readonly Value[]): string[] {
  return cells.map((c) => cellText(c));
}

/** A tab or a newline inside a cell would break a TSV grid, so it becomes a space. */
function tsvLine(cells: readonly string[]): string {
  return cells.map((c) => c.replace(/[\t\r\n]+/g, ' ')).join('\t');
}

/** Tab-separated, one line per row. */
function tsv(titles: readonly string[], rows: readonly (readonly string[])[]): string {
  return [tsvLine(titles), ...rows.map(tsvLine)].join('\n');
}

/** The row selection the bar keeps level with the table. */
export interface Picking {
  mode: TableBlock['select'];
  chosen: Set<string>;
  /** Every row's checkbox, to tick to match `chosen`. */
  boxes: readonly { box: HTMLInputElement; key: string }[];
  selectAll: HTMLInputElement;
}

/** The bar for `block`, and the function that brings the checkboxes and
 *  counts level with the selection after it changes. `rows` is the table's
 *  own list, reordered in place when it sorts, so a copy writes the order
 *  the person sees. */
export function tableBar(
  block: TableBlock,
  agent: ViewCtx,
  rows: readonly BarRow[],
  picking: Picking,
): { bar: HTMLElement; sync: () => void } {
  const { mode, chosen, boxes, selectAll } = picking;
  const picked = el('span', { class: 'vtable-count' });
  const shownCount = el('span', { class: 'vtable-count' });
  const buttons = block.actions.map((action) =>
    actionButton(action, agent, () => ({
      rows: rows.flatMap((r) => (r.key !== null && chosen.has(r.key) ? [r.key] : [])),
    })),
  );

  const sync = (): void => {
    for (const { box, key } of boxes) box.checked = chosen.has(key);
    const visible = rows.filter((r) => r.shown && r.key !== null);
    const all = visible.length > 0 && visible.every((r) => r.key !== null && chosen.has(r.key));
    selectAll.checked = all;
    selectAll.indeterminate = !all && visible.some((r) => r.key !== null && chosen.has(r.key));
    picked.textContent = mode === 'none' ? '' : `${chosen.size} selected`;
    for (const b of buttons)
      (b as HTMLButtonElement).disabled = mode !== 'none' && chosen.size === 0;
  };

  const apply = (needle: string): void => {
    const q = needle.trim().toLowerCase();
    for (const r of rows) {
      r.shown = !q || r.text.includes(q);
      r.tr.hidden = !r.shown;
      if (r.actions) r.actions.hidden = !r.shown;
      if (r.detail) r.detail.hidden = !r.shown;
    }
    shownCount.textContent = q
      ? `${rows.filter((r) => r.shown).length} of ${rows.length} rows`
      : '';
    sync();
  };

  const search =
    block.rows.length > FILTERABLE
      ? el('input', {
          type: 'search',
          class: 'vtable-filter',
          placeholder: 'Filter rows',
          'aria-label': 'Filter rows',
        })
      : null;
  if (search) {
    search.value = filterOf(agent.path);
    search.addEventListener('input', () => {
      setFilter(agent.path, search.value);
      apply(search.value);
    });
  }

  const bar = el(
    'div',
    { class: 'vtable-bar' },
    search,
    shownCount,
    picked,
    el('span', { class: 'vtable-bar-gap' }),
    buttons,
    copyButton('Copy as TSV', () =>
      tsv(
        block.columns.map((c) => c.title),
        rows.filter((r) => r.shown).map((r) => rowText(r.cells)),
      ),
    ),
  );
  apply(search ? search.value : '');
  return { bar, sync };
}
