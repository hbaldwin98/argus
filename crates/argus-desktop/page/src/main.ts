// The desktop window's page: it draws what Rust sends and reports what the
// person does. It decides nothing about the daemon — the model is the
// window's `session` — so each event replaces what the page held.

import type { Event } from './bindings/Event';
import { chime, listen as listenForSound } from './chime.js';
import {
  closeStart,
  drawChrome,
  drawHeading,
  notice,
  railPicked,
  reconnect,
  unpickRail,
} from './chrome.js';
import { drawCheckouts } from './checkouts.js';
import { $ } from './dom.js';
import { drawHistory } from './history.js';
import { drawReview } from './review.js';
import { drawFeatures, listen as listenToFeatures } from './features.js';
import { listen as listenToInput } from './input.js';
import { listen as listenToKeys } from './keymap.js';
import { screen } from './screen.js';
import { state } from './state.js';
import { call, connect } from './tauri.js';

function receive(event: Event): void {
  switch (event.type) {
    case 'status':
      state.status = event;
      drawChrome();
      break;
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
      if (event.stage !== before.stage && event.stage === 'feature') $('input').blur();
      else if (railPicked()) return;
      else if (shown || (event.shown && event.stage !== before.stage)) $('input').focus();
      break;
    }
    case 'frame':
      screen.frame(event);
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
      break;
    case 'review':
      state.review = event;
      drawReview();
      break;
    case 'checkouts':
      state.checkouts = event;
      drawCheckouts();
      break;
    case 'features':
      state.features = event;
      drawFeatures();
      break;
    case 'chime':
      chime();
      break;
    case 'keymap':
      state.keymap = event.groups;
      break;
  }
}

listenToInput();
listenForSound();
listenToKeys();
listenToFeatures();

document.addEventListener('click', (e) => {
  // A click anywhere but its own items closes a checkout's + menu.
  closeStart();
  if (state.menuOpen && e.target instanceof Node && !$('heading').contains(e.target)) {
    state.menuOpen = false;
    drawHeading();
  }
});
// A press anywhere but the rail lets go of its picked row.
document.addEventListener('mousedown', (e) => {
  if (e.target instanceof Node && !$('rail').contains(e.target)) unpickRail();
});
$('reconnect').addEventListener('click', reconnect);
$('tab-checkouts').addEventListener('click', () => call('show_checkouts', { open: true }));

drawChrome();
connect(receive);
