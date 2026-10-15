// What every block of an agent's view is drawn with: which view it is in,
// so an action or a reference inside it knows whom to ask, and the path of
// the block, which keys what the page alone remembers about it (a chosen
// tab, an opened row) across a redraw.

import type { PageDiagram } from './bindings/PageDiagram';

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
  /** The paths, under the view (`2`, `1.0.3`), of the blocks that changed
   *  since the person saw them; those are drawn marked. */
  changed?: ReadonlySet<string>;
}

/** The context of the block at `index` under `ctx`. */
export function at(ctx: ViewCtx, index: number | string): ViewCtx {
  return { ...ctx, path: `${ctx.path}.${index}` };
}
