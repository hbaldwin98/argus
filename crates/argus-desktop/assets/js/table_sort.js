// How a table's rows sort when a header is clicked: what a cell sorts by,
// how two of those compare, and the arrows that say which column is sorted.
/** What a cell sorts by: figures and times by magnitude, the rest by text. */
function sortKey(v) {
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
        case 'unknown':
            return '';
    }
}
function compare(a, b) {
    if (typeof a === 'number' && typeof b === 'number') {
        // An unparseable date is NaN; keep those last rather than scrambling.
        if (Number.isNaN(a) || Number.isNaN(b)) {
            return Number(Number.isNaN(a)) - Number(Number.isNaN(b));
        }
        return a - b;
    }
    return String(a).localeCompare(String(b));
}
/** `rows` ordered by column `c`; rows that compare equal keep the order they
 *  were first drawn in, so sorting twice does not shuffle ties. */
export function sortRows(rows, c, direction) {
    const sign = direction === 'ascending' ? 1 : -1;
    const keyed = rows.map((r) => ({ r, key: r.cells[c] ? sortKey(r.cells[c]) : '' }));
    keyed.sort((x, y) => sign * compare(x.key, y.key) || x.r.index - y.r.index);
    return keyed.map((k) => k.r);
}
/** Marks which header is sorted, and which way. `other` is a column the
 *  agent sorted by without saying the direction. */
export function markHeaders(headers, active, dir) {
    headers.forEach((th, c) => {
        const on = c === active;
        th.setAttribute('aria-sort', on ? dir : 'none');
        const arrow = th.querySelector('.vtable-arrow');
        if (arrow)
            arrow.textContent = !on ? '' : dir === 'ascending' ? '↑' : '↓';
    });
}
