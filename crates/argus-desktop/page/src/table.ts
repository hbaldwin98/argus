// A table block: the agent's rows as a real table that scrolls sideways in a
// narrow panel, and re-sorts client-side when a header is clicked.

import type { Block } from './bindings/Block';
import type { Value } from './bindings/Value';
import { el } from './dom.js';
import { value } from './values.js';

export type TableBlock = Extract<Block, { kind: 'table' }>;

type Direction = 'ascending' | 'descending';

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
    case 'empty':
      return '';
  }
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

export function table(block: TableBlock): HTMLElement {
  if (block.rows.length === 0) return el('div', { class: 'vtable-empty' }, 'No rows');

  const rows = block.rows.map((cells, index) => {
    const tr = el(
      'tr',
      null,
      block.columns.map((col, c) => {
        const cell = cells[c];
        const td = el('td', { class: col.numeric ? 'vtable-num' : null });
        if (cell) {
          td.appendChild(value(cell));
          // Long text truncates in CSS, so the full text rides along as a tooltip.
          if (cell.kind === 'text' && cell.text.length > 24) {
            td.classList.add('vtable-long');
            td.title = cell.text;
          }
        }
        return td;
      }),
    );
    return { tr, cells, index };
  });

  const body = el(
    'tbody',
    null,
    rows.map((r) => r.tr),
  );
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
    const keyed = rows.map((r) => ({ r, key: r.cells[c] ? sortKey(r.cells[c]) : '' }));
    keyed.sort((x, y) => sign * compare(x.key, y.key) || x.r.index - y.r.index);
    body.append(...keyed.map((k) => k.r.tr));
    mark(active, direction);
  };

  const head = el(
    'tr',
    null,
    block.columns.map((col, c) => {
      const th = el(
        'th',
        { scope: 'col', class: col.numeric ? 'vtable-num' : null },
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
  if (block.total === null || block.total <= block.rows.length) return wrap;
  return el(
    'div',
    { class: 'vtable-block' },
    wrap,
    el('div', { class: 'vtable-more' }, `Showing ${block.rows.length} of ${block.total} rows`),
  );
}
