// A table block: the agent's rows as a real table that scrolls sideways in a
// narrow panel, and re-sorts client-side when a header is clicked. An agent's
// table can also select rows, offer actions on them, open a row to blocks,
// filter, and copy itself as TSV; an extension's table has a key per row and
// a selected one instead (`RowPicks`) and none of those.

import type { Block } from './bindings/Block';
import type { Value } from './bindings/Value';
import { actionButton } from './actions.js';
import { blocks } from './blocks.js';
import { cellText } from './cell_text.js';
import { copyButton } from './copy.js';
import { el, icon, type Child } from './dom.js';
import { filterOf, openRows, selection, setFilter } from './table_memory.js';
import { value } from './values.js';
import { at, type ViewCtx } from './viewctx.js';

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

type Direction = 'ascending' | 'descending';

/** Past this many rows a table offers a filter. */
const FILTERABLE = 8;

/** What a cell sorts by: figures and times by magnitude, the rest by text. */
function sortKey(v: Value): number | string {
  switch (v.kind) {
    case 'number':
      return v.number;
    case 'date':
      return Date.parse(v.date);
    case 'date_time':
      return Date.parse(v.at);
    case 'text':
      return v.text.toLowerCase();
    case 'bool':
      return v.value ? 1 : 0;
    case 'person':
      return v.name.toLowerCase();
    case 'people':
      return v.names.join(' ').toLowerCase();
    case 'chip':
      return v.chip.label.toLowerCase();
    case 'chips':
      return v.chips
        .map((c) => c.label)
        .join(' ')
        .toLowerCase();
    case 'link':
      return v.label.toLowerCase();
    case 'ticket':
      return v.key.toLowerCase();
    case 'progress':
      return v.value / v.max;
    case 'duration':
      return v.ms;
    case 'code':
      return v.text.toLowerCase();
    case 'path':
      return v.path.toLowerCase();
    case 'commit':
      return v.sha.toLowerCase();
    case 'feature':
      return v.number;
    case 'pane':
      return (v.label ?? String(v.pane)).toLowerCase();
    case 'empty':
      return '';
  }
}

function classes(col: TableBlock['columns'][number]): string | null {
  const names = [col.numeric && 'vtable-num', col.optional && 'vtable-optional'];
  return names.filter(Boolean).join(' ') || null;
}

function compare(a: number | string, b: number | string): number {
  if (typeof a === 'number' && typeof b === 'number') {
    // An unparseable date is NaN; keep those last rather than scrambling.
    if (Number.isNaN(a) || Number.isNaN(b)) {
      return Number(Number.isNaN(a)) - Number(Number.isNaN(b));
    }
    return a - b;
  }
  return String(a).localeCompare(String(b));
}

/** A row's cells as the text a filter matches and a copy writes. */
function rowText(cells: readonly Value[]): string[] {
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
  const openable = (r: number): boolean => (block.details[r]?.length ?? 0) > 0;
  const hasDetails = agent !== null && block.rows.some((_, r) => openable(r));
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
      cells.push(el('td', { class: 'vtable-open' }, chevron(row)));
    }
    return cells;
  };

  const rowId = (row: Row): string => row.key ?? `#${row.index}`;

  const chevron = (row: Row): Node | null => {
    if (!openable(row.index)) return null;
    const button = el(
      'button',
      { type: 'button', class: 'vtable-chev', 'aria-label': 'Show row details' },
      icon('right'),
    );
    const show = (): void => {
      const isOpen = open.has(rowId(row));
      button.setAttribute('aria-expanded', String(isOpen));
      button.classList.toggle('open', isOpen);
      row.detail?.remove();
      row.detail = null;
      const detail = block.details[row.index];
      if (isOpen && agent && detail) {
        row.detail = el(
          'tr',
          { class: 'vtable-detail' },
          el(
            'td',
            { colspan: String(block.columns.length + leading) },
            blocks(detail, at(agent, `d${row.index}`)),
          ),
        );
      }
      // A row not yet in the table is placed with the rest by the caller.
      if (row.detail) row.tr.after(row.detail);
    };
    button.addEventListener('click', () => {
      const id = rowId(row);
      if (!open.delete(id)) open.add(id);
      show();
    });
    row.refresh = show;
    return button;
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

  const mark = (active: number | null, dir: Direction | 'other'): void => {
    headers.forEach((th, c) => {
      const on = c === active;
      th.setAttribute('aria-sort', on ? dir : 'none');
      const arrow = th.querySelector('.vtable-arrow');
      if (arrow) arrow.textContent = !on ? '' : dir === 'ascending' ? '↑' : '↓';
    });
  };

  let active: number | null = block.sorted_by;
  let direction: Direction = 'descending';

  const sortBy = (c: number): void => {
    direction = active === c && direction === 'ascending' ? 'descending' : 'ascending';
    active = c;
    const sign = direction === 'ascending' ? 1 : -1;
    const sorted = rows.map((r) => ({ r, key: r.cells[c] ? sortKey(r.cells[c]) : '' }));
    sorted.sort((x, y) => sign * compare(x.key, y.key) || x.r.index - y.r.index);
    // The order the person sees is the order a copy writes.
    rows.splice(0, rows.length, ...sorted.map((s) => s.r));
    body.append(...rows.flatMap(lines));
    mark(active, direction);
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
  mark(active, 'other');

  const wrap = el(
    'div',
    { class: 'vtable' },
    el('table', { class: 'vtable-table' }, el('thead', null, head), body),
  );

  const parts: HTMLElement[] = [];
  if (agent) {
    const picked = el('span', { class: 'vtable-count' });
    const shownCount = el('span', { class: 'vtable-count' });
    const buttons = block.actions.map((action) =>
      actionButton(action, agent, () => ({
        rows: rows.flatMap((r) => (r.key !== null && chosen.has(r.key) ? [r.key] : [])),
      })),
    );

    const filtering = block.rows.length > FILTERABLE;
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

    sync = (): void => {
      for (const { box, key } of boxes) box.checked = chosen.has(key);
      const visible = rows.filter((r) => r.shown && r.key !== null);
      const all = visible.length > 0 && visible.every((r) => r.key !== null && chosen.has(r.key));
      selectAll.checked = all;
      selectAll.indeterminate = !all && visible.some((r) => r.key !== null && chosen.has(r.key));
      picked.textContent = mode === 'none' ? '' : `${chosen.size} selected`;
      for (const b of buttons)
        (b as HTMLButtonElement).disabled = mode !== 'none' && chosen.size === 0;
    };

    const search = filtering
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

    parts.push(
      el(
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
      ),
    );
    apply(search ? search.value : '');
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

/** The key a row's open state is remembered by: the agent's, else its place. */
function rowIds(block: TableBlock): string[] {
  const keyed = block.row_keys.length === block.rows.length;
  return block.rows.map((_, r) => (keyed ? (block.row_keys[r] ?? `#${r}`) : `#${r}`));
}
