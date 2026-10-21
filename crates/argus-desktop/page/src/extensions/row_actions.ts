// When a table row shows its actions, and which of them the dock's ⋯ menu
// lists with a key. A row shows them while it is the selected one or the
// pointer is on it, so a keyboard user, whose selection moves with the
// keys, always has them.

import type { ExtTableAction } from '../bindings/ExtTableAction';

export function showsActions(
  key: string | null,
  selected: string | null,
  hovered: string | null,
): boolean {
  return key !== null && (key === selected || key === hovered);
}

/** The actions that answer to a key, listed in the menu that now carries
 *  the keys in place of a key cap on every chip. */
export function keyed(actions: readonly ExtTableAction[]): ExtTableAction[] {
  return actions.filter((a) => a.key);
}
