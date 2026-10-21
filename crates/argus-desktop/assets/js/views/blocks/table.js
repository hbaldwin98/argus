// A table block: the agent's rows as a real table that scrolls sideways in a
// narrow panel, and re-sorts client-side when a header is clicked. An agent's
// table can also select rows, offer actions on them, open a row to blocks,
// filter, and copy itself as TSV; an extension's table has a key per row and
// a selected one instead (`RowPicks`) and none of those. The bar over an
// agent's table is `table/bar`'s, a row opening to blocks `table/details`'s,
// and the sorting `table/sort`'s; this builds the table they work on.
import { el } from '../../kit/dom.js';
import { showsActions } from '../../extensions/row_actions.js';
import { dotted } from '../chip.js';
import { rowText, tableBar } from './table/bar.js';
import { chevron, openable, rowIds } from './table/details.js';
import { openRows, selection } from './table/memory.js';
import { markHeaders, sortRows } from './table/sort.js';
import { value } from '../values.js';
/** How many of an extension's trailing columns give way, last first, as the
 *  table narrows: its columns are listed most important first. */
const DROPPABLE = 4;
function classes(col, drop) {
    const names = [
        col.numeric && 'vtable-num',
        col.optional && 'vtable-optional',
        drop > 0 && `vtable-drop-${drop}`,
    ];
    return names.filter(Boolean).join(' ') || null;
}
const nothing = () => { };
export function table(block, picks = null, ctx = null) {
    if (block.rows.length === 0)
        return el('div', { class: 'vtable-empty' }, 'No rows');
    // An agent's table has a context and no picks; the page's extras are its.
    const agent = picks === null && ctx !== null ? ctx : null;
    const keyed = block.row_keys.length === block.rows.length;
    const mode = agent && keyed ? block.select : 'none';
    const chosen = agent && mode !== 'none' ? selection(agent.path, block.row_keys) : new Set();
    const hasDetails = agent !== null && block.rows.some((_, r) => openable(block, r));
    const open = agent && hasDetails ? openRows(agent.path, rowIds(block)) : new Set();
    const leading = (mode !== 'none' ? 1 : 0) + (hasDetails ? 1 : 0);
    // Where the cell is for the nth column from the end that may be dropped; the
    // first two columns, and the status chip leading them, always stay.
    const dropOf = (c) => {
        if (picks === null)
            return 0;
        const rank = block.columns.length - c;
        return c >= 2 && rank <= DROPPABLE ? rank : 0;
    };
    const rows = block.rows.map((cells, index) => {
        const key = picks?.keys[index] ?? (keyed ? (block.row_keys[index] ?? null) : null);
        const on = picks !== null && key !== null && key === picks.selected;
        const tr = el('tr', picks && key !== null
            ? {
                class: on ? 'on' : null,
                'aria-selected': String(on),
                onclick: () => picks.select(key),
                ondblclick: picks.activate && (() => picks.activate?.(key)),
            }
            : null);
        const row = {
            tr,
            detail: null,
            cells,
            text: rowText(cells).join('\n').toLowerCase(),
            index,
            key,
            shown: true,
            refresh: null,
        };
        return row;
    });
    const body = el('tbody', null);
    const boxes = [];
    let sync = nothing;
    // The checkbox or radio, and the chevron, before the first cell.
    const lead = (row) => {
        const cells = [];
        if (mode !== 'none' && row.key !== null) {
            const key = row.key;
            const box = el('input', {
                type: mode === 'many' ? 'checkbox' : 'radio',
                name: mode === 'one' ? `pick-${agent?.path}` : null,
                'aria-label': 'Select row',
            });
            box.checked = chosen.has(key);
            box.addEventListener('change', () => {
                if (mode === 'one')
                    chosen.clear();
                if (box.checked)
                    chosen.add(key);
                else
                    chosen.delete(key);
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
        row.tr.append(...lead(row), ...block.columns.map((col, c) => {
            const cell = row.cells[c];
            const td = el('td', { class: classes(col, dropOf(c)) });
            if (cell) {
                td.appendChild(picks?.dots && cell.kind === 'chip' ? dotted(cell.chip) : value(cell, ctx));
                // Long text truncates in CSS, so the full text rides along as a tooltip.
                if (cell.kind === 'text' && cell.text.length > 24) {
                    td.classList.add('vtable-long');
                    td.title = cell.text;
                }
            }
            return td;
        }));
        // The buttons of the selected row, and of the one under the pointer, sit
        // at the row's end, pinned to the table's window, so nothing is inserted
        // to push the rows below down. Built only while shown: a row at rest has
        // none in the document, and the selected row always has them, so what
        // the keys reach is never mouse-only.
        if (picks?.trailing && row.key !== null) {
            const key = row.key;
            const trailing = picks.trailing;
            const cell = el('td', { class: 'vtable-acts' });
            const bar = () => {
                if (!cell.firstChild)
                    cell.append(el('div', { class: 'vtable-acts-bar' }, trailing(key)));
            };
            if (showsActions(key, picks.selected, null))
                bar();
            row.tr.addEventListener('pointerenter', bar);
            row.tr.addEventListener('pointerleave', () => {
                if (!showsActions(key, picks.selected, null) && !cell.contains(document.activeElement)) {
                    cell.replaceChildren();
                }
            });
            row.tr.append(cell);
        }
    }
    const lines = (r) => [r.tr, r.detail].filter((n) => n !== null);
    for (const r of rows)
        r.refresh?.();
    body.append(...rows.flatMap(lines));
    const headers = [];
    let active = block.sorted_by;
    let direction = 'descending';
    const sortBy = (c) => {
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
            if (r.key === null || !r.shown)
                continue;
            if (selectAll.checked)
                chosen.add(r.key);
            else
                chosen.delete(r.key);
        }
        sync();
    });
    const head = el('tr', null, mode !== 'none'
        ? el('th', { class: 'vtable-pick', scope: 'col' }, mode === 'many' ? selectAll : null)
        : null, hasDetails ? el('th', { class: 'vtable-open', scope: 'col' }) : null, block.columns.map((col, c) => {
        const th = el('th', { scope: 'col', class: classes(col, dropOf(c)) }, el('button', { type: 'button', class: 'vtable-sort', onclick: () => sortBy(c) }, col.title, el('span', { class: 'vtable-arrow', 'aria-hidden': 'true' })));
        headers.push(th);
        return th;
    }), picks?.trailing
        ? el('th', { class: 'vtable-acts', scope: 'col', 'aria-label': 'Actions' })
        : null);
    // The agent says it sorted this column but not which way, so "other"
    // rather than a descending we would be guessing at.
    markHeaders(headers, active, 'other');
    const wrap = el('div', { class: 'vtable' }, el('table', { class: 'vtable-table' }, el('thead', null, head), body));
    const parts = [];
    if (agent) {
        const built = tableBar(block, agent, rows, { mode, chosen, boxes, selectAll });
        sync = built.sync;
        parts.push(built.bar);
    }
    parts.push(wrap);
    if (block.total !== null && block.total > block.rows.length) {
        parts.push(el('div', { class: 'vtable-more' }, `Showing ${block.rows.length} of ${block.total} rows`));
    }
    if (parts.length === 1)
        return wrap;
    return el('div', { class: 'vtable-block' }, parts);
}
