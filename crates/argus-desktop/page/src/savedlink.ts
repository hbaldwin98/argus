// A link from an element of a view to a saved view. Following one asks
// Rust to open the target: as the next column of the saved panel the link
// was followed in, or, from an agent's view or an extension's table, as a
// panel of its own. The page never decides where; it says which column the
// press was in. A target the window does not hold is drawn as missing, and
// still pressed, since the view may be saved later.

import type { PageSaved } from './bindings/PageSaved';
import { el } from './kit/dom.js';
import { state } from './state.js';
import { call } from './tauri.js';
import type { ViewCtx } from './viewctx.js';

/** The saved view this window holds under `id`. */
export function savedView(id: string): PageSaved | undefined {
  return state.saved?.views.find((v) => v.id === id);
}

export function openSaved(id: string, ctx: ViewCtx | null): void {
  call('open_saved_view', { id, from: ctx?.saved ?? null });
}

/** A saved view named by a `view` value: its label, else its title. */
export function savedLink(
  id: string,
  label: string | null | undefined,
  ctx: ViewCtx | null,
): HTMLElement {
  const held = savedView(id);
  return el(
    'button',
    {
      class: held ? 'vref vsaved' : 'vref vsaved vsaved-missing',
      type: 'button',
      title: held ? `Open the saved view ${held.view.title}` : `No saved view ${id} yet`,
      onclick: (event: MouseEvent) => {
        event.stopPropagation();
        openSaved(id, ctx);
      },
    },
    label ?? held?.view.title ?? id,
  );
}

/** The small button beside an element that opens a saved view, for the
 *  elements labelled by plain text. Nothing when the element opens none. */
export function opensMark(id: string | null | undefined, ctx: ViewCtx | null): HTMLElement | false {
  if (id == null) return false;
  const held = savedView(id);
  const name = held ? held.view.title : id;
  return el(
    'button',
    {
      class: held ? 'vopen' : 'vopen vsaved-missing',
      type: 'button',
      title: held ? `Open ${name}` : `No saved view ${id} yet`,
      'aria-label': `Open the saved view ${name}`,
      onclick: (event: MouseEvent) => {
        event.preventDefault();
        event.stopPropagation();
        openSaved(id, ctx);
      },
    },
    '↗',
  );
}
