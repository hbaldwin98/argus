// What the page holds: the last of each event Rust sent, and the little
// view state that is the page's own — a menu open, a filter typed.

import type { Features } from './bindings/Features';
import type { Rail } from './bindings/Rail';
import type { Status } from './bindings/Status';

export const state: {
  status: Status;
  rail: Rail;
  features: Features | null;
  /** The project switcher's menu. */
  menuOpen: boolean;
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
  },
  features: null,
  menuOpen: false,
  filter: '',
  archivedOpen: false,
};
