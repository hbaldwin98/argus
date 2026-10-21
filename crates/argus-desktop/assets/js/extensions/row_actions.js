// When a table row shows its actions, and which of them the dock's ⋯ menu
// lists with a key. A row shows them while it is the selected one or the
// pointer is on it, so a keyboard user, whose selection moves with the
// keys, always has them.
export function showsActions(key, selected, hovered) {
    return key !== null && (key === selected || key === hovered);
}
/** The actions that answer to a key, listed in the menu that now carries
 *  the keys in place of a key cap on every chip. */
export function keyed(actions) {
    return actions.filter((a) => a.key);
}
