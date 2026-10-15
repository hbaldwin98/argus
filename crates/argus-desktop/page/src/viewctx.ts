// What every block of an agent's view is drawn with: which view it is in,
// so an action or a reference inside it knows whom to ask, and the path of
// the block, which keys what the page alone remembers about it (a chosen
// tab, an opened row) across a redraw.

import type { PageDiagram } from './bindings/PageDiagram';
import type { SavedColumn } from './bindings/SavedColumn';

/** A diagram block's source as Rust read it, to draw. */
export type Drawn = (source: string) => PageDiagram | undefined;

export interface ViewCtx {
  /** The pane whose agent showed the view. */
  pane: number;
  /** The view's id. */
  id: string;
  /** The block's path under the view, `pane/id.2.0`. */
  path: string;
  drawn: Drawn;
  /** Ids of the view's actions pressed and not yet answered by the agent
   *  showing the view again; their buttons wait. */
  pressed?: readonly string[];
  /** Whether the daemon takes a button press; a button is disabled when not. */
  canPress: boolean;
  /** The paths, under the view (`2`, `1.0.3`), of the blocks that changed
   *  since the person saw them; those are drawn marked. */
  changed?: ReadonlySet<string>;
  /** The column of a saved panel this is drawn in, which a link followed
   *  here gets its target the column after. Absent in an agent's view and
   *  an extension's table, whose links open a panel of their own. */
  saved?: SavedColumn;
  /** The saved views being drawn, outermost first: set for a saved view,
   *  whose actions and references are not an agent pane's. An embed of one
   *  of these is a cycle. */
  chain?: readonly string[];
  /** How many embeds were followed to reach this view. */
  depth?: number;
}

/** The pane whose agent showed the view, for a reference in it to be read
 *  against; a saved view or an extension's table has none. */
export function paneOf(ctx: ViewCtx | null): number | null {
  return ctx && !ctx.chain ? ctx.pane : null;
}

/** The context of the block at `index` under `ctx`. */
export function at(ctx: ViewCtx, index: number | string): ViewCtx {
  return { ...ctx, path: `${ctx.path}.${index}` };
}
