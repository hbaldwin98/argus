// A table block: the agent's rows as a real table that scrolls sideways in a
// narrow panel, and re-sorts client-side when a header is clicked. An agent's
// table can also select rows, offer actions on them, open a row to blocks,
// filter, and copy itself as TSV; an extension's table has a key per row and
// a selected one instead (`RowPicks`) and none of those. The bar over an
// agent's table is `table_bar`'s, a row opening to blocks `table_details`'s,
// and the sorting `table_sort`'s; this builds the table they work on.

import type { Block } from './bindings/Block';
import type { Value } from './bindings/Value';
import { el, type Child } from './dom.js';
import { rowText, tableBar } from './table_bar.js';
import { chevron, openable, rowIds } from './table_details.js';
import { openRows, selection } from './table_memory.js';
import { markHeaders, sortRows, type Direction } from './table_sort.js';
import { value } from './values.js';
import type { ViewCtx } from './viewctx.js';

export type TableBlock = Extract<Block, { kind: 'table' }>;

/** What an extension's view adds to a table: a key per row, the one
 *  selected, what a click and a double-click on a row do, and what the
 *  selected row offers on a line under it. An agent's table has none of it. */
export interface RowPicks {
  keys: readonly string[];
  selected: string | null;
  select: (key: string) => void;
  activate: ((key: string) => void) | null;
  trailing: ((key: string) => Child) | null;
}

function classes(col: TableBlock['columns'][number]): string | null {
  const names = [col.numeric && 'vtable-num', col.optional && 'vtable-optional'];
  return names.filter(Boolean).join(' ') || null;
}

const nothing = (): void => {};

interface Row {
  tr: HTMLElement;
  /** An extension's selected-row buttons, a row of their own under it. */
  actions: HTMLElement | null;
  /** The block list this row opens to, drawn only while it is open. */
  detail: HTMLElement | null;
  cells: readonly Value[];
  text: string;
  index: number;
  /** The agent's key for the row, when it gave one. */
  key: string | null;
  /** Whether the row is among those the filter lets through. */
  shown: boolean;
  /** Draws or removes the opened detail to match whether the row is open. */
  refresh: (() => void) | null;
}

export function table(
  block: TableBlock,
  picks: RowPicks | null = null,
  ctx: ViewCtx | null = null,
): HTMLElement {
  if (block.rows.length === 0) return el('div', { class: 'vtable-empty' }, 'No rows');

  // An agent's table has a context and no picks; the page's extras are its.
  const agent = picks === null && ctx !== null ? ctx : null;
  const keyed = block.row_keys.length === block.rows.length;
  const mode = agent && keyed ? block.select : 'none';
  const chosen =
    agent && mode !== 'none' ? selection(agent.path, block.row_keys) : new Set<string>();
  const hasDetails = agent !== null && block.rows.some((_, r) => openable(block, r));
  const open = agent && hasDetails ? openRows(agent.path, rowIds(block)) : new Set<string>();
  const leading = (mode !== 'none' ? 1 : 0) + (hasDetails ? 1 : 0);

  const rows: Row[] = block.rows.map((cells, index) => {
    const key = picks?.keys[index] ?? (keyed ? (block.row_keys[index] ?? null) : null);
    const on = picks !== null && key !== null && key === picks.selected;
    const tr = el(
      'tr',
      picks && key !== null
        ? {
            class: on ? 'on' : null,
            'aria-selected': String(on),
            onclick: () => picks.select(key),
            ondblclick: picks.activate && (() => picks.activate?.(key)),
          }
        : null,
    );
    const row: Row = {
      tr,
      actions: null,
      detail: null,
      cells,
      text: rowText(cells).join('\n').toLowerCase(),
      index,
      key,
      shown: true,
      refresh: null,
    };
    // A row of its own, so a long run of buttons wraps instead of widening
    // the table, and moves with its row when the table is sorted.
    if (on && key !== null && picks?.trailing) {
      row.actions = el(
        'tr',
        { class: 'row-actions' },
        el(
          'td',
          { colspan: String(block.columns.length) },
          el('div', { class: 'row-actions-bar' }, picks.trailing(key)),
        ),
      );
    }
    return row;
  });

  const body = el('tbody', null);
  const boxes: { box: HTMLInputElement; key: string }[] = [];
  let sync = nothing;

  // The checkbox or radio, and the chevron, before the first cell.
  const lead = (row: Row): Node[] => {
    const cells: Node[] = [];
    if (mode !== 'none' && row.key !== null) {
      const key = row.key;
      const box = el('input', {
        type: mode === 'many' ? 'checkbox' : 'radio',
        name: mode === 'one' ? `pick-${agent?.path}` : null,
        'aria-label': 'Select row',
      });
      box.checked = chosen.has(key);
      box.addEventListener('change', () => {
        if (mode === 'one') chosen.clear();
        if (box.checked) chosen.add(key);
        else chosen.delete(key);
        sync();
      });
      boxes.push({ box, key });
      cells.push(el('td', { class: 'vtable-pick' }, box));
    }
    if (hasDetails) {
      cells.push(el('td', { class: 'vtable-open' }, chevron(row, block, agent, open, leading)));
    }
    return cells;
  };

  for (const row of rows) {
    row.tr.append(
      ...lead(row),
      ...block.columns.map((col, c) => {
        const cell = row.cells[c];
        const td = el('td', { class: classes(col) });
        if (cell) {
          td.appendChild(value(cell, ctx));
          // Long text truncates in CSS, so the full text rides along as a tooltip.
          if (cell.kind === 'text' && cell.text.length > 24) {
            td.classList.add('vtable-long');
            td.title = cell.text;
          }
        }
        return td;
      }),
    );
  }

  const lines = (r: Row): HTMLElement[] =>
    [r.tr, r.actions, r.detail].filter((n): n is HTMLElement => n !== null);
  for (const r of rows) r.refresh?.();
  body.append(...rows.flatMap(lines));

  const headers: HTMLTableCellElement[] = [];

  let active: number | null = block.sorted_by;
  let direction: Direction = 'descending';

  const sortBy = (c: number): void => {
    direction = active === c && direction === 'ascending' ? 'descending' : 'ascending';
    active = c;
    // The order the person sees is the order a copy writes.
    rows.splice(0, rows.length, ...sortRows(rows, c, direction));
    body.append(...rows.flatMap(lines));
    markHeaders(headers, active, direction);
  };

  const selectAll = el('input', { type: 'checkbox', 'aria-label': 'Select all rows' });
  selectAll.addEventListener('change', () => {
    for (const r of rows) {
      if (r.key === null || !r.shown) continue;
      if (selectAll.checked) chosen.add(r.key);
      else chosen.delete(r.key);
    }
    sync();
  });

  const head = el(
    'tr',
    null,
    mode !== 'none'
      ? el('th', { class: 'vtable-pick', scope: 'col' }, mode === 'many' ? selectAll : null)
      : null,
    hasDetails ? el('th', { class: 'vtable-open', scope: 'col' }) : null,
    block.columns.map((col, c) => {
      const th = el(
        'th',
        { scope: 'col', class: classes(col) },
        el(
          'button',
          { type: 'button', class: 'vtable-sort', onclick: () => sortBy(c) },
          col.title,
          el('span', { class: 'vtable-arrow', 'aria-hidden': 'true' }),
        ),
      );
      headers.push(th);
      return th;
    }),
  );
  // The agent says it sorted this column but not which way, so "other"
  // rather than a descending we would be guessing at.
  markHeaders(headers, active, 'other');

  const wrap = el(
    'div',
    { class: 'vtable' },
    el('table', { class: 'vtable-table' }, el('thead', null, head), body),
  );

  const parts: HTMLElement[] = [];
  if (agent) {
    const built = tableBar(block, agent, rows, { mode, chosen, boxes, selectAll });
    sync = built.sync;
    parts.push(built.bar);
  }

  parts.push(wrap);
  if (block.total !== null && block.total > block.rows.length) {
    parts.push(
      el('div', { class: 'vtable-more' }, `Showing ${block.rows.length} of ${block.total} rows`),
    );
  }
  if (parts.length === 1) return wrap;
  return el('div', { class: 'vtable-block' }, parts);
}
