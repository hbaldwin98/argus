// What the page holds: the last of each event Rust sent, and the little
// view state that is the page's own — a menu open, a draft typed.

import type { AddProjectPage } from './bindings/AddProjectPage';
import type { CapturePage } from './bindings/CapturePage';
import type { Checkouts } from './bindings/Checkouts';
import type { CodePage } from './bindings/CodePage';
import type { Command } from './bindings/Command';
import type { ConsolePage } from './bindings/ConsolePage';
import type { DocksPage } from './bindings/DocksPage';
import type { ExtensionsPage } from './bindings/ExtensionsPage';
import type { Features } from './bindings/Features';
import type { Group } from './bindings/Group';
import type { KeyLayer } from './bindings/KeyLayer';
import type { Rail } from './bindings/Rail';
import type { SavedViewsPage } from './bindings/SavedViewsPage';
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
  | { what: 'addDecision'; under: number | null; task: number | null; text: string }
  | { what: 'supersede'; id: number; text: string }
  | { what: 'addFlow'; text: string }
  | { what: 'flow'; id: number; title: string; text: string }
  | { what: 'step'; diagram: number; step: number; text: string };

/** The selected flow's diagram over the whole window: how it is sized, and
 *  whether the steps are listed beside it. The page's own, like a draft. */
export type Expanded = { zoom: number; fit: boolean; panel: boolean };

export const state: {
  status: Status;
  rail: Rail;
  features: Features | null;
  /** The note a new feature is being written as. */
  capture: CapturePage | null;
  editing: Editing | null;
  /** The option picked on an open decision, before it is taken. */
  chosen: { id: number; option: number } | null;
  /** The flow's diagram expanded over the window, while it is. */
  expanded: Expanded | null;
  /** Every binding, as `src/keys/keymap.rs` groups them. */
  keymap: Group[];
  /** What the palette offers to run, from the same table. */
  commands: Command[];
  /** Who has a key outside a pane, the first up of them, in this order. */
  layers: KeyLayer[];
  /** What the palette can go to, while it is open. */
  palette: Palette | null;
  /** The project switcher's menu, and its workspaces unfolded. */
  menuOpen: boolean;
  workspacesOpen: boolean;
  /** The hosts unfolded under Connect to a host…. */
  hostsOpen: boolean;
  addProject: AddProjectPage | null;
  /** The checkout whose + menu is open, where it opened from, and its
   *  highlighted item. */
  start: { checkout: number; from: 'rail' | 'table'; at: number } | null;
  /** The rail's row last clicked, which holds the keys until a click
   *  elsewhere: a checkout, or a pane and its checkout. */
  railPick: { checkout: number; pane: number | null } | null;
  checkouts: Checkouts | null;
  code: CodePage | null;
  review: ReviewPage | null;
  history: HistoryPage | null;
  sessions: SessionsPage | null;
  /** The Settings dialog's choices, which the window's look follows. */
  settings: SettingsPage | null;
  /** Where the console is, its tabs, and whether the canvas is its. */
  console: ConsolePage | null;
  /** Each dock's tabs, the one it shows, and how big it is. */
  docks: DocksPage | null;
  /** What the extensions put in the window: views, commands, keys. */
  extensions: ExtensionsPage | null;
  /** The views agents saved, and the columns of each panel showing them. */
  saved: SavedViewsPage | null;
  /** The composer's draft: its picks and message, kept across redraws
   *  until a pane it started is made. */
  compose: { checkout: number | null; template: string | null; text: string };
} = {
  status: { connection: 'connecting', detail: null, alarm: null, chime: true, host: null },
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
    workspaceShown: true,
    templates: [],
    workspaces: [],
    hosts: [],
    removing: null,
    width: null,
    outlineWidth: null,
    briefHeight: null,
    codeTreeWidth: null,
    codeOutlineWidth: null,
    row: {
      beside: [],
      panels: [],
      grid: [[0]],
      heights: [1000],
      widths: [[1000]],
      keyed: null,
      minCols: 20,
      minRows: 6,
    },
  },
  features: null,
  capture: null,
  editing: null,
  chosen: null,
  expanded: null,
  keymap: [],
  commands: [],
  layers: [],
  palette: null,
  menuOpen: false,
  workspacesOpen: false,
  hostsOpen: false,
  addProject: null,
  start: null,
  railPick: null,
  checkouts: null,
  code: null,
  review: null,
  history: null,
  sessions: null,
  settings: null,
  console: null,
  docks: null,
  extensions: null,
  saved: null,
  compose: { checkout: null, template: null, text: '' },
};
