// What the page holds: the last of each event Rust sent, and the little
// view state that is the page's own — a menu open, a filter typed.

import type { Checkouts } from './bindings/Checkouts';
import type { Command } from './bindings/Command';
import type { Features } from './bindings/Features';
import type { Group } from './bindings/Group';
import type { Rail } from './bindings/Rail';
import type { ReviewPage } from './bindings/ReviewPage';
import type { HistoryPage } from './bindings/HistoryPage';
import type { Palette } from './bindings/Palette';
import type { Status } from './bindings/Status';

export const state: {
  status: Status;
  rail: Rail;
  features: Features | null;
  /** Every binding, as `src/keymap.rs` groups them. */
  keymap: Group[];
  /** What the palette offers to run, from the same table. */
  commands: Command[];
  /** What the palette can go to, while it is open. */
  palette: Palette | null;
  /** The project switcher's menu. */
  menuOpen: boolean;
  /** The checkout whose + menu is open, where it opened from, and its
   *  highlighted item. */
  start: { checkout: number; from: 'rail' | 'table'; at: number } | null;
  /** The rail's row last clicked, which holds the keys until a click
   *  elsewhere: a checkout, or a pane and its checkout. */
  railPick: { checkout: number; pane: number | null } | null;
  checkouts: Checkouts | null;
  review: ReviewPage | null;
  history: HistoryPage | null;
  /** The comment being written, kept across the dialog's redraws. */
  reviewDraft: string;
  /** What the index's filter says. */
  filter: string;
  /** Whether the board's archived column is unfolded. */
  archivedOpen: boolean;
} = {
  status: { connection: 'connecting', detail: null, alarm: null },
  rail: {
    projects: [],
    project: null,
    needsYou: 0,
    working: 0,
    shown: null,
    loaded: false,
    repository: null,
    stage: 'workspace',
    filled: false,
    templates: [],
  },
  features: null,
  keymap: [],
  commands: [],
  palette: null,
  menuOpen: false,
  start: null,
  railPick: null,
  checkouts: null,
  review: null,
  history: null,
  reviewDraft: '',
  filter: '',
  archivedOpen: false,
};
