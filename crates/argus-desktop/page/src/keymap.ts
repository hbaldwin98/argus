// The keys pressed outside a pane, looked up in the keymap Rust sends
// (`src/keymap.rs`) and run here. A pane's keys go to Rust whole, and its
// leader is dispatched there.

import type { Action } from './bindings/Action';
import type { Chord } from './bindings/Chord';
import type { Place } from './bindings/Place';
import { answerNo, checkoutsAction } from './checkouts.js';
import { focusHistoryFilter, historyOpen } from './history.js';
import { reviewOpen } from './review.js';
import { focusSessionsSearch, sessionsOpen } from './sessions.js';
import { railAction, railPicked, startAction, startOpen } from './chrome.js';
import { $ } from './dom.js';
import { closeLayer, featureAction } from './features.js';
import { closePalette, openPalette } from './palette.js';
import { closeSheet, openSheet, sheetOpen, toggleSheet } from './shortcuts.js';
import { state } from './state.js';
import { call } from './tauri.js';

/** `Chord::matches` again: the page decides before Rust hears the key. */
function matches(chord: Chord, e: KeyboardEvent): boolean {
  return (
    e.key === chord.key &&
    ([...chord.key].length === 1 || e.shiftKey === chord.shift) &&
    e.ctrlKey === chord.ctrl &&
    e.altKey === chord.alt &&
    e.metaKey === chord.meta
  );
}

/** What `e` does in `places`, leaving aside keys pressed after the leader. */
export function actionFor(places: Place[], e: KeyboardEvent): Action | null {
  for (const group of state.keymap) {
    if (!places.includes(group.place)) continue;
    for (const binding of group.bindings) {
      if (binding.chords.some((chord) => !chord.leader && matches(chord, e))) return binding.action;
    }
  }
  return null;
}

/** Runs `action`, saying whether there was anything for it to do. */
function run(action: Action): boolean {
  const layerOpen = $('layer').childElementCount > 0;
  if (action === 'closeLayer') {
    // A question the stage asked is answered no, not only put away.
    if (answerNo()) return true;
    if (layerOpen) closeLayer();
    return layerOpen;
  }
  // A menu or dialog over the stage holds the keys until it closes.
  if (layerOpen) return false;
  switch (action) {
    case 'palette':
    case 'shortcuts':
      runOver(action);
      return true;
    case 'workspace':
    case 'feature':
      call('show_features', { open: action === 'feature' });
      return true;
    case 'checkouts':
      call('show_checkouts', { open: true });
      return true;
    case 'compose':
      call('show_composer', { open: true });
      return true;
    default:
      return state.rail.stage === 'checkouts' ? checkoutsAction(action) : featureAction(action);
  }
}

/** Opens the palette or the sheet, for a key here or one Rust heard in a
 *  pane: one over the other would leave two holding the keys. */
export function runOver(action: Action): void {
  if (action === 'palette') {
    closeSheet();
    openPalette();
  } else if (action === 'shortcuts') {
    closePalette();
    toggleSheet();
  }
}

/** The keys a dialog does not answer, open over it. */
function notTheirs(action: Action): boolean {
  return (
    action === 'workspace' ||
    action === 'feature' ||
    action === 'checkouts' ||
    action === 'compose' ||
    action === 'palette'
  );
}

export function listen(): void {
  document.addEventListener('keydown', (e) => {
    // A text field takes its own keys, and the pane's text area sends its
    // keys to Rust.
    const typing =
      e.target instanceof Element && e.target.closest('input, textarea, [contenteditable]');
    if (typing) return;
    // The sheet is over everything and holds the keys until put away.
    if (sheetOpen()) {
      const action = actionFor(['anywhere'], e);
      if (action === 'closeLayer' || action === 'shortcuts') {
        e.preventDefault();
        closeSheet();
      }
      return;
    }
    // The review, history and sessions dialogs hold the keys while open,
    // all but the sheet's, which lists theirs; Rust runs the rest.
    if (reviewOpen()) {
      const action = actionFor(['anywhere', 'review'], e);
      if (action === 'shortcuts') {
        e.preventDefault();
        openSheet();
      } else if (action && !notTheirs(action)) {
        e.preventDefault();
        call('review_action', { action });
      }
      return;
    }
    if (historyOpen()) {
      const action = actionFor(['anywhere', 'history'], e);
      if (action === 'shortcuts') {
        e.preventDefault();
        openSheet();
      } else if (action && !notTheirs(action)) {
        e.preventDefault();
        if (action === 'filter') focusHistoryFilter();
        else call('history_action', { action, row: null });
      }
      return;
    }
    if (sessionsOpen()) {
      const action = actionFor(['anywhere', 'sessions'], e);
      if (action === 'shortcuts') {
        e.preventDefault();
        openSheet();
      } else if (action && !notTheirs(action)) {
        e.preventDefault();
        if (action === 'filter') focusSessionsSearch();
        else call('sessions_action', { action, row: null });
      }
      return;
    }
    // An open + menu holds the keys until it closes.
    if (startOpen()) {
      const action = actionFor(['anywhere', 'start'], e);
      if (action && startAction(action)) e.preventDefault();
      return;
    }
    // A row picked in the rail holds the keys until a click elsewhere.
    if (railPicked()) {
      const action = actionFor(['anywhere', 'rail'], e);
      if (action && (railAction(action) || run(action))) e.preventDefault();
      return;
    }
    const places: Place[] =
      state.rail.stage === 'feature'
        ? ['anywhere', 'feature', 'card']
        : state.rail.stage === 'checkouts'
          ? ['anywhere', 'checkouts']
          : ['anywhere'];
    const action = actionFor(places, e);
    if (action && run(action)) e.preventDefault();
  });
}
