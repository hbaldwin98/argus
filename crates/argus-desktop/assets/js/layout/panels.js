// What Rust says of each panel that exists (`src/layout/docks.rs`): which module
// draws it, and whether it floats, takes the stage or closes for good.
// The page asks here, and never reads any of it from a panel's name.
import { state } from '../state.js';
/** What the docks said of `panel`, or `null` for one that does not exist
 *  now, which can only be docked. */
export function panelOf(panel) {
    return state.docks?.present.find((p) => p.panel === panel) ?? null;
}
