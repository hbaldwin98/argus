// Which layer of the window has a key outside a pane, asked in the order
// Rust sends with the keymap (`LAYERS` in `src/keymap.rs`). The page keeps
// no order of its own, only what each layer does; it reads no key itself,
// so it imports nothing but types and is tested under node.

import type { Action } from './bindings/Action';
import type { KeyLayer } from './bindings/KeyLayer';
import type { Layer } from './bindings/Layer';
import type { Place } from './bindings/Place';

/** What one layer does while it has the keys. A layer with no places
 *  answers its own keys and needs none of the rest. */
export interface Handler {
  /** Whether the layer is up, the keys being at `target`. */
  open(target: EventTarget | null): boolean;
  /** A dialog's: runs the action it answers. */
  sink?: (action: Action) => void;
  /** A dialog's: gives its filter or search field the keys. */
  filter?: () => void;
  /** A layer's own handling of a key, in place of a dialog's, saying
   *  whether it held it; one it lets go is heard by the next layer up. */
  keys?: (e: KeyboardEvent, places: Place[]) => boolean;
  /** What Escape puts away here, saying whether there was anything. */
  escape?: () => boolean;
}

export type Handlers = Record<Layer, Handler>;

/** The layers up, the one with the keys first. Lazy, since most keys stop
 *  at the first. */
export function* up(order: readonly KeyLayer[], handlers: Handlers, target: EventTarget | null) {
  for (const layer of order) {
    if (handlers[layer.layer].open(target)) yield layer;
  }
}

/** Whether a layer that takes the keys from a pane is up. */
export function takesFromPane(
  order: readonly KeyLayer[],
  handlers: Handlers,
  target: EventTarget | null,
): boolean {
  return order.some((layer) => layer.takesFromPane && handlers[layer.layer].open(target));
}

/** Puts away the first thing Escape finds, in the layers' order. Each hook
 *  is asked whether its layer is up or not: a question outlives the menu
 *  drawn over it, and is still answered no. */
export function escapeFrom(order: readonly KeyLayer[], handlers: Handlers): boolean {
  return order.some((layer) => handlers[layer.layer].escape?.() === true);
}
