// A table row that opens to blocks of its own: the chevron that opens it
// and the row of blocks drawn under it while it is open. Which rows are open
// is `table_memory`'s, so a redraw of the view leaves them open.
import { blocks } from './blocks.js';
import { el, icon } from './kit/dom.js';
import { at } from './viewctx.js';
/** The key a row's open state is remembered by: the agent's, else its place. */
export function rowIds(block) {
    const keyed = block.row_keys.length === block.rows.length;
    return block.rows.map((_, r) => (keyed ? (block.row_keys[r] ?? `#${r}`) : `#${r}`));
}
export function openable(block, r) {
    return (block.details[r]?.length ?? 0) > 0;
}
/** The chevron that opens `row`, or none if it has nothing to open to.
 *  `leading` is how many cells (a checkbox, the chevron) come before the
 *  row's own, so the detail spans the whole row. */
export function chevron(row, block, agent, open, leading) {
    if (!openable(block, row.index))
        return null;
    const id = row.key ?? `#${row.index}`;
    const button = el('button', { type: 'button', class: 'vtable-chev', 'aria-label': 'Show row details' }, icon('right'));
    const show = () => {
        const isOpen = open.has(id);
        button.setAttribute('aria-expanded', String(isOpen));
        button.classList.toggle('open', isOpen);
        row.detail?.remove();
        row.detail = null;
        const detail = block.details[row.index];
        if (isOpen && agent && detail) {
            // The path the page names this detail's blocks by is `Slot::page_step`'s.
            row.detail = el('tr', { class: 'vtable-detail' }, el('td', { colspan: String(block.columns.length + leading) }, blocks(detail, at(agent, `d${row.index}`))));
        }
        // A row not yet in the table is placed with the rest by the caller.
        if (row.detail)
            row.tr.after(row.detail);
    };
    button.addEventListener('click', () => {
        if (!open.delete(id))
            open.add(id);
        show();
    });
    row.refresh = show;
    return button;
}
