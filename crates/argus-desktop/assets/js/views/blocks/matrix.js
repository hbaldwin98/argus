// A matrix view block: a grid of tinted cells.
import { toneClass } from '../chip.js';
import { el } from '../../kit/dom.js';
export function matrix(block) {
    return el('div', { class: 'vmatrix-wrap' }, el('table', { class: 'vmatrix' }, el('thead', null, el('tr', null, el('td', null), block.columns.map((column) => el('th', { scope: 'col' }, column)))), el('tbody', null, block.rows.map((row, r) => el('tr', null, el('th', { scope: 'row' }, row), block.columns.map((column, c) => {
        const cell = block.cells[r]?.[c];
        if (!cell)
            return el('td', { class: 'vmatrix-cell vmatrix-none' });
        // The tint rises from a faint wash to most of the tone, so the
        // lightest cell is still a cell and the darkest keeps its text.
        const weight = Math.min(1, Math.max(0, cell.weight));
        const label = cell.label ?? '';
        return el('td', {
            class: `vmatrix-cell ${toneClass(cell.tone)}`,
            style: `--w:${Math.round(weight * 80 + 10)}%`,
            title: `${row} · ${column}${label && `: ${label}`}`,
        }, label);
    }))))));
}
