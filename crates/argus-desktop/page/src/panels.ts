// What Rust says of each panel that exists (`src/docks.rs`): which module
// draws it, and whether it floats, takes the stage or closes for good.
// The page asks here, and never reads any of it from a panel's name.

import type { Panel } from './bindings/Panel';
import type { PresentPanel } from './bindings/PresentPanel';
import { state } from './state.js';

/** What the docks said of `panel`, or `null` for one that does not exist
 *  now, which can only be docked. */
export function panelOf(panel: Panel): PresentPanel | null {
  return state.docks?.present.find((p) => p.panel === panel) ?? null;
}
