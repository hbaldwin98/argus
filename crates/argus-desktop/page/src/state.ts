// What the page holds: the last of each event Rust sent, and the little
// view state that is the page's own — a menu open, a filter typed.

import type { AddProjectPage } from './bindings/AddProjectPage';
import type { CapturePage } from './bindings/CapturePage';
import type { Checkouts } from './bindings/Checkouts';
import type { Command } from './bindings/Command';
import type { ConsolePage } from './bindings/ConsolePage';
import type { Features } from './bindings/Features';
import type { Group } from './bindings/Group';
import type { Rail } from './bindings/Rail';
import type { ReviewPage } from './bindings/ReviewPage';
import type { HistoryPage } from './bindings/HistoryPage';
import type { SessionsPage } from './bindings/SessionsPage';
import type { SettingsPage } from './bindings/SettingsPage';
import type { Palette } from './bindings/Palette';
import type { Status } from './bindings/Status';

/** A form being written over an open feature, and what it holds so far: one
 *  at a time, kept across the redraws a push makes. */
export type Editing =
  | { what: 'brief'; text: string }
  | { what: 'task'; id: number; title: string; body: string }
  | { what: 'record'; id: number; text: string }
  | { what: 'answer'; id: number; text: string; because: string }
  | { what: 'criterion'; id: number; text: string }
  | { what: 'addTask'; parent: number | null; text: string }
  | { what: 'addDecision'; under: number | null; task: number | null; text: string };

export const state: {
  status: Status;
  rail: Rail;
  features: Features | null;
  /** The note a new feature is being written as. */
  capture: CapturePage | null;
  editing: Editing | null;
  /** The option picked on an open decision, before it is taken. */
  chosen: { id: number; option: number } | null;
  /** Every binding, as `src/keymap.rs` groups them. */
  keymap: Group[];
  /** What the palette offers to run, from the same table. */
  commands: Command[];
  /** What the palette can go to, while it is open. */
  palette: Palette | null;
  /** The project switcher's menu, and its workspaces unfolded. */
  menuOpen: boolean;
  workspacesOpen: boolean;
  addProject: AddProjectPage | null;
  /** The checkout whose + menu is open, where it opened from, and its
   *  highlighted item. */
  start: { checkout: number; from: 'rail' | 'table'; at: number } | null;
  /** The rail's row last clicked, which holds the keys until a click
   *  elsewhere: a checkout, or a pane and its checkout. */
  railPick: { checkout: number; pane: number | null } | null;
  checkouts: Checkouts | null;
  review: ReviewPage | null;
  history: HistoryPage | null;
  sessions: SessionsPage | null;
  /** The Settings dialog's choices, which the window's look follows. */
  settings: SettingsPage | null;
  /** Where the console is, its tabs, and whether the canvas is its. */
  console: ConsolePage | null;
  /** The comment being written, kept across the dialog's redraws. */
  reviewDraft: string;
  /** What the index's filter says. */
  filter: string;
  /** Whether the board's archived column is unfolded. */
  archivedOpen: boolean;
  /** The composer's draft: its picks and message, kept across redraws
   *  until a pane it started is made. */
  compose: { checkout: number | null; template: string | null; text: string };
} = {
  status: { connection: 'connecting', detail: null, alarm: null, chime: true },
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
    workspaces: [],
    removing: null,
  },
  features: null,
  capture: null,
  editing: null,
  chosen: null,
  keymap: [],
  commands: [],
  palette: null,
  menuOpen: false,
  workspacesOpen: false,
  addProject: null,
  start: null,
  railPick: null,
  checkouts: null,
  review: null,
  history: null,
  sessions: null,
  settings: null,
  console: null,
  reviewDraft: '',
  filter: '',
  archivedOpen: false,
  compose: { checkout: null, template: null, text: '' },
};
