// Which layer of the window has a key outside a pane, asked in the order
// Rust sends with the keymap (`LAYERS` in `src/keymap.rs`). The page keeps
// no order of its own, only what each layer does; it reads no key itself,
// so it imports nothing but types and is tested under node.
/** The layers up, the one with the keys first. Lazy, since most keys stop
 *  at the first. */
export function* up(order, handlers, target) {
    for (const layer of order) {
        if (handlers[layer.layer].open(target))
            yield layer;
    }
}
/** Whether a layer that takes the keys from a pane is up. */
export function takesFromPane(order, handlers, target) {
    return order.some((layer) => layer.takesFromPane && handlers[layer.layer].open(target));
}
/** Puts away the first thing Escape finds, in the layers' order. Each hook
 *  is asked whether its layer is up or not: a question outlives the menu
 *  drawn over it, and is still answered no. */
export function escapeFrom(order, handlers) {
    return order.some((layer) => handlers[layer.layer].escape?.() === true);
}
