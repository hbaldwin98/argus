// What the console's tab strip offers, as plain data: which tabs wear a
// status dot, and the one menu that holds what used to be four icons
// (split, move, fill the window). `console` turns it into buttons.

import type { StatusWord } from '../bindings/StatusWord';

/** An idle shell is the ordinary case, so it wears nothing; a dot is for a
 *  console that is doing something, wants someone or went wrong. */
export function wearsDot(status: StatusWord): boolean {
  return status !== 'idle';
}

export type StripAction = 'split' | 'tab' | 'dock' | 'fullscreen';

export interface StripItem {
  action: StripAction;
  label: string;
}

/** The ⋯ menu's items for the console as it stands: split or put the split
 *  away, move between the Console stage and its dock, fill the window or
 *  leave it. `dock` is how the dock is named: "at the bottom". */
export function stripItems(on: {
  split: boolean;
  docked: boolean;
  full: boolean;
  dock: string;
}): StripItem[] {
  return [
    { action: 'split', label: on.split ? 'Put the split away' : 'Split' },
    on.docked
      ? { action: 'tab', label: 'Open in the Console stage' }
      : { action: 'dock', label: `Dock ${on.dock}` },
    { action: 'fullscreen', label: on.full ? 'Leave fullscreen' : 'Fullscreen' },
  ];
}
