// What the page alone remembers about an agent's table across a redraw of
// its view: the rows selected, the rows opened, the filter typed. Keyed by
// the block's path, so another view's table never shares them; a key that
// no longer names a row is let go when the table is drawn again.
const selected = new Map();
const opened = new Map();
const filters = new Map();
function setOf(map, path, keys) {
    let set = map.get(path);
    if (!set)
        map.set(path, (set = new Set()));
    for (const k of set)
        if (!keys.includes(k))
            set.delete(k);
    return set;
}
/** The selected row keys of the table at `path`, trimmed to `keys`. */
export function selection(path, keys) {
    return setOf(selected, path, keys);
}
/** The opened rows of the table at `path`, trimmed to `keys`. */
export function openRows(path, keys) {
    return setOf(opened, path, keys);
}
export function filterOf(path) {
    return filters.get(path) ?? '';
}
export function setFilter(path, text) {
    if (text)
        filters.set(path, text);
    else
        filters.delete(path);
}
