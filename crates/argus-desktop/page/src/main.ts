// The desktop window's page: it draws what Rust sends and reports what the
// person does. It decides nothing about the daemon — the model is the
// window's `session` — so each event replaces what the page held.

import type { Event } from './bindings/Event';
import { chime, listen as listenForSound } from './system/chime.js';
import {
  closeStart,
  drawChrome,
  drawHead,
  listenToRailEdge,
  notice,
  railPicked,
  reconnect,
  unpickRail,
} from './chrome.js';
import { drawCheckouts } from './checkouts/stage.js';
import { codeSettings, drawCode, editEvent } from './code/stage.js';
import { drawDocks, listen as listenToDocks } from './layout/docks.js';
import { clearComposer, focusComposer } from './agents/compose.js';
import { $ } from './kit/dom.js';
import { drawHistory } from './review/history.js';
import { drawLink } from './board/tracker/link.js';
import { drawReconcile } from './board/tracker/reconcile.js';
import { drawReview } from './review/dialog.js';
import { drawSessions } from './agents/sessions_dialog.js';
import { drawSettings } from './settings/preferences.js';
import { drawFeatures, listen as listenToFeatures } from './board/feature_stage.js';
import { listen as listenToInput } from './panes/input.js';
import { dialogTakesKeys, listen as listenToKeys, runOver } from './keys/keymap.js';
import { listen as listenToMenus } from './kit/menu.js';
import { listen as listenToFrame } from './system/frame.js';
import { drawPalette, openPalette } from './keys/palette.js';
import { drawAddProject, drawHeading } from './chrome/projects.js';
import { drawFrame, screen } from './panes/screen.js';
import { state } from './state.js';
import { call, connect } from './connection/tauri.js';
import { drawViews, openNewestView } from './views/agent.js';
import { browserShot, browsersHeard, drawBrowserPanel } from './browser/stage.js';
import { drawExtensionDialog } from './extensions/dialogs.js';
import { drawExtensionConfirm, drawExtensionPanels } from './extensions/views.js';
import { drawSavedNav, drawSavedPanels } from './views/saved.js';

/** Whether the pane had the keys when a dialog opened over it. */
let paneHadKeys = false;

/** A dialog opened from a pane (leader-Tab) takes the keys from it, and
 *  gives them back once no dialog is left open. */
function dialogKeys(): void {
  const input = $('input');
  if (dialogTakesKeys()) {
    if (document.activeElement === input) {
      paneHadKeys = true;
      input.blur();
    }
  } else if (paneHadKeys) {
    paneHadKeys = false;
    if (state.rail.shown) input.focus();
  }
}

function receive(event: Event): void {
  switch (event.type) {
    case 'status':
      state.status = event;
      drawChrome();
      break;
    case 'docks':
      state.docks = event;
      drawDocks();
      drawViews();
      drawBrowserPanel();
      // Which saved panel is out marks its entry in the sidebar.
      drawSavedNav();
      break;
    case 'console': {
      const before = state.console?.typing ?? null;
      state.console = event;
      drawChrome();
      // A console newly on screen takes the keys, as a pane does, and so
      // does a split side given them by a press on its header.
      if (event.typing !== null && event.typing !== before) $('input').focus();
      break;
    }
    case 'rail': {
      const before = state.rail;
      state.rail = event;
      screen.expect(event.shown?.pane ?? null);
      drawChrome();
      document.body.classList.toggle('filled', event.filled);
      // A pane newly on screen takes the keys, however it got there: a
      // click on its row, or a shell started from a checkout's +. So does
      // the pane coming back with the Workspace, and leaving it for the
      // Feature stage lets the keys go to the stage. A row picked in the
      // rail keeps them, the pane it shows or the one after a close.
      const shown = event.shown && event.shown.pane !== before.shown?.pane;
      // The composer's start was made: its draft is done with.
      if (before.stage === 'new' && event.stage === 'workspace' && shown) clearComposer();
      if (event.stage !== before.stage && event.stage === 'new') {
        focusComposer();
        return;
      }
      if (event.stage !== before.stage && event.stage === 'feature') $('input').blur();
      else if (railPicked()) return;
      else if (shown || (event.shown && event.stage !== before.stage)) $('input').focus();
      break;
    }
    case 'frame':
      drawFrame(event);
      break;
    case 'clipboard':
      navigator.clipboard?.writeText(event.text).catch(() => {});
      break;
    case 'notice':
      notice(event.text);
      break;
    case 'history':
      state.history = event;
      drawHistory();
      dialogKeys();
      break;
    case 'sessions':
      state.sessions = event;
      drawSessions();
      dialogKeys();
      break;
    case 'addProject':
      state.addProject = event;
      drawAddProject();
      dialogKeys();
      break;
    case 'review':
      state.review = event;
      drawReview();
      dialogKeys();
      break;
    case 'link':
      drawLink(event);
      dialogKeys();
      break;
    case 'reconcile':
      drawReconcile(event);
      dialogKeys();
      break;
    case 'checkouts':
      state.checkouts = event;
      drawCheckouts();
      // The counts and buttons live in the strip.
      drawHead();
      break;
    case 'code':
      state.code = event;
      drawCode();
      break;
    case 'edit':
      editEvent(event);
      break;
    case 'features':
      state.features = event;
      drawFeatures();
      // An open feature names the strip.
      drawHead();
      break;
    case 'capture':
      state.capture = event;
      drawFeatures();
      break;
    case 'extensions':
      state.extensions = event;
      drawExtensionPanels();
      drawChrome();
      drawExtensionConfirm();
      break;
    case 'extensionDialog':
      drawExtensionDialog(event);
      break;
    case 'views':
      drawViews(event);
      // A view open as a page names the strip.
      drawHead();
      // A ticket link says when a view of it waits.
      drawFeatures();
      break;
    case 'browsers':
      browsersHeard(event);
      // The chosen browser's agent names the strip.
      drawHead();
      break;
    case 'browserShot':
      browserShot(event);
      break;
    case 'savedViews':
      state.saved = event;
      drawSavedPanels();
      drawSavedNav();
      // The views of a feature are listed on its stage.
      drawFeatures();
      break;
    case 'chime':
      chime();
      break;
    case 'settings':
      state.settings = event;
      drawSettings();
      codeSettings();
      break;
    case 'keymap':
      state.keymap = event.groups;
      state.commands = event.commands;
      state.layers = event.layers;
      drawChrome();
      break;
    case 'palette':
      state.palette = event;
      drawPalette();
      break;
    case 'run':
      runOver(event.action);
      break;
  }
}

listenToFrame();
listenToInput();
listenForSound();
listenToKeys();
listenToFeatures();
listenToDocks();
listenToMenus();
listenToRailEdge();

document.addEventListener('click', (e) => {
  // A click anywhere but its own items closes a checkout's + menu.
  closeStart();
  // By the path the click took, not where its target is now: the heading
  // redraws under its own clicks, which takes the target out of it.
  if (state.menuOpen && !e.composedPath().includes($('heading'))) {
    state.menuOpen = false;
    state.workspacesOpen = false;
    state.hostsOpen = false;
    drawHeading();
  }
});
// A press anywhere but the rail lets go of its picked row.
document.addEventListener('mousedown', (e) => {
  if (e.target instanceof Node && !$('rail').contains(e.target)) unpickRail();
});
$('reconnect').addEventListener('click', reconnect);
$('views-tab').addEventListener('click', openNewestView);
$('nav-search').addEventListener('click', openPalette);
$('nav-new').addEventListener('click', () => call('show_composer', { open: true }));
$('nav-workspace').addEventListener('click', () => call('show_features', { open: false }));
$('nav-feature').addEventListener('click', () => call('show_features', { open: true }));
$('nav-checkouts').addEventListener('click', () => call('show_checkouts', { open: true }));
$('nav-code').addEventListener('click', () => call('show_code', { open: true }));
$('nav-console').addEventListener('click', () => call('console', { gesture: { on: 'tab' } }));
$('nav-browsers').addEventListener('click', () => call('show_browsers', { pane: null }));

drawChrome();
connect(receive);
