// The keys pressed outside a pane, looked up in the keymap Rust sends
// (`src/keymap.rs`) and run here. A pane's keys go to Rust whole, and its
// leader is dispatched there.

import type { Action } from './bindings/Action';
import type { Chord } from './bindings/Chord';
import type { Place } from './bindings/Place';
import { captureOpen, discardCapture } from './capture.js';
import { answerNo, checkoutsAction } from './checkouts.js';
import { focusHistoryFilter, historyOpen } from './history.js';
import { linkAction, linkOpen } from './link.js';
import { reconcileAction, reconcileOpen } from './reconcile.js';
import { reviewOpen } from './review.js';
import { focusSessionsSearch, readerOpen, sessionsOpen } from './sessions.js';
import { focusTranscriptFind } from './transcript.js';
import { railAction, railPicked, startAction, startOpen } from './chrome.js';
import { extensionDialogFocused } from './dialogs.js';
import { $ } from './dom.js';
import { answerExtensionNo, extensionViewFocused, runExtensionCommand } from './extension_views.js';
import { expandedOpen } from './sequence.js';
import { featureAction } from './features.js';
import { closeLayer } from './menu.js';
import { closePalette, openPalette } from './palette.js';
import { addProjectOpen, answerRemoveNo } from './projects.js';
import { closeSettings, openSettings, settingsOpen } from './settings.js';
import { closeSheet, openSheet, sheetOpen, toggleSheet } from './shortcuts.js';
import { state } from './state.js';
import { call } from './tauri.js';
import { leaveWork, workAction, workOpen } from './work.js';

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

/** What `e` does in `places`, leaving aside keys pressed after the leader.
 *  The places are asked in the order given, so one listed first may take a
 *  key from one after it: an open feature's digits are its lenses. */
export function actionFor(places: Place[], e: KeyboardEvent): Action | null {
  for (const place of places) {
    const group = state.keymap.find((g) => g.place === place);
    for (const binding of group?.bindings ?? []) {
      if (binding.chords.some((chord) => !chord.leader && matches(chord, e))) return binding.action;
    }
  }
  return null;
}

/** The id of the action on the extension view out whose key `e` is: one
 *  character, typed with no modifier but its own shift. */
function extensionActionFor(e: KeyboardEvent): string | null {
  if (e.ctrlKey || e.altKey || e.metaKey) return null;
  const actions = state.extensions?.open?.actions ?? [];
  return actions.find((action) => action.key !== null && action.key === e.key)?.id ?? null;
}

type Scope = 'anywhere' | 'view' | 'checkout' | 'pane';

/** Runs the extension command whose key `e` is among those heard in
 *  `scopes`, saying whether there was one. The daemon refused any key the
 *  keymap holds, so none of these shadows one of the window's own. */
function extensionCommand(e: KeyboardEvent, scopes: Scope[]): boolean {
  const view = state.extensions?.open?.view ?? null;
  const command = (state.extensions?.commands ?? []).find(
    (c) =>
      scopes.includes(c.scope) &&
      (c.scope !== 'view' || c.view === view) &&
      c.chords.some((chord) => matches(chord, e)),
  );
  if (!command) return false;
  e.preventDefault();
  runExtensionCommand(command.extension, command.command);
  return true;
}

/** Runs `action`, saying whether there was anything for it to do. */
function run(action: Action): boolean {
  const layerOpen = $('layer').childElementCount > 0;
  if (action === 'closeLayer') {
    // A question the stage asked is answered no, not only put away.
    if (answerNo() || answerRemoveNo() || answerExtensionNo()) return true;
    if (layerOpen) {
      closeLayer();
      return true;
    }
    // The note discards, and an open feature goes back to the index.
    if (state.rail.stage === 'feature' && captureOpen()) {
      discardCapture();
      return true;
    }
    if (workOpen()) {
      leaveWork();
      return true;
    }
    return false;
  }
  // A menu or dialog over the stage holds the keys until it closes.
  if (layerOpen) return false;
  switch (action) {
    case 'palette':
    case 'shortcuts':
    case 'settings':
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
    case 'console':
      call('console', { gesture: { on: 'tab' } });
      return true;
    case 'toggleConsole':
      call('console', { gesture: { on: 'toggle' } });
      return true;
    case 'consoleFullscreen':
      call('console', { gesture: { on: 'fullscreen' } });
      return true;
    case 'newConsole':
      call('console', { gesture: { on: 'spawn' } });
      return true;
    case 'extensionPage':
    case 'extensionDock':
      call('extension_mode', { page: action === 'extensionPage' });
      return true;
    case 'views':
      call('toggle_views', {});
      return true;
    default:
      if (state.rail.stage === 'checkouts') return checkoutsAction(action);
      if (state.rail.stage === 'extension') {
        const moves =
          action === 'next' || action === 'previous' || action === 'first' || action === 'last';
        if (moves) call('extension_key', { action });
        return moves;
      }
      return workOpen() ? workAction(action) : featureAction(action);
  }
}

/** Opens the palette, the sheet or the settings, for a key here or one
 *  Rust heard in a pane: one over another would leave two holding the
 *  keys. */
export function runOver(action: Action): void {
  if (action === 'palette') {
    closeSheet();
    closeSettings();
    openPalette();
  } else if (action === 'shortcuts') {
    closePalette();
    closeSettings();
    toggleSheet();
  } else if (action === 'settings') {
    closePalette();
    closeSheet();
    openSettings();
  }
}

/** The keys a dialog does not answer, open over it. */
function notTheirs(action: Action): boolean {
  return (
    action === 'workspace' ||
    action === 'feature' ||
    action === 'checkouts' ||
    action === 'compose' ||
    action === 'console' ||
    action === 'toggleConsole' ||
    action === 'views' ||
    action === 'palette' ||
    action === 'settings'
  );
}

export function listen(): void {
  // The window's keys come first, while the event is still on its way
  // down: a pane's text area, a field or a dialog never hears them.
  document.addEventListener(
    'keydown',
    (e) => {
      const action = actionFor(['window'], e);
      if (!action) return;
      e.preventDefault();
      e.stopPropagation();
      call('command', { action });
    },
    { capture: true },
  );
  document.addEventListener('keydown', (e) => {
    // A text field takes its own keys, and the pane's text area sends its
    // keys to Rust.
    const typing =
      e.target instanceof Element && e.target.closest('input, textarea, [contenteditable]');
    if (typing) return;
    // An extension's question answers its own keys while it has them.
    if (extensionDialogFocused()) return;
    // The settings answer their own keys, Escape among them.
    if (settingsOpen()) return;
    // The sheet is over everything and holds the keys until put away.
    if (sheetOpen()) {
      const action = actionFor(['anywhere'], e);
      if (action === 'closeLayer' || action === 'shortcuts') {
        e.preventDefault();
        closeSheet();
      }
      return;
    }
    // The add-a-project dialog answers its own keys.
    if (addProjectOpen()) return;
    // The tracker dialogs hold the keys while open, as the review does.
    if (linkOpen() || reconcileOpen()) {
      const link = linkOpen();
      const action = actionFor(['anywhere', link ? 'link' : 'reconcile'], e);
      if (action === 'shortcuts') {
        e.preventDefault();
        openSheet();
      } else if (action && !notTheirs(action)) {
        e.preventDefault();
        if (link) linkAction(action);
        else reconcileAction(action);
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
    // The full transcript, open in place of the list, holds its own.
    if (sessionsOpen()) {
      const reading = readerOpen();
      const action = actionFor(['anywhere', reading ? 'transcript' : 'sessions'], e);
      if (action === 'shortcuts') {
        e.preventDefault();
        openSheet();
      } else if (action && !notTheirs(action)) {
        e.preventDefault();
        if (action === 'filter') {
          if (reading) focusTranscriptFind();
          else focusSessionsSearch();
        } else call('sessions_action', { action, row: null });
      }
      return;
    }
    // An open + menu holds the keys until it closes.
    if (startOpen()) {
      const action = actionFor(['anywhere', 'start'], e);
      if (action && startAction(action)) e.preventDefault();
      return;
    }
    // An extension's view clicked holds its own keys and its actions'.
    if (extensionViewFocused() && $('layer').childElementCount === 0) {
      const action = actionFor(['extension'], e);
      const id = action ? null : extensionActionFor(e);
      if (action === 'extensionPage' || action === 'extensionDock') run(action);
      else if (action) call('extension_key', { action });
      else if (id) call('extension_action', { action: id });
      if (action || id) {
        e.preventDefault();
        return;
      }
      if (extensionCommand(e, ['view', 'anywhere'])) return;
    }
    // A row picked in the rail holds the keys until a click elsewhere.
    if (railPicked()) {
      const action = actionFor(['anywhere', 'rail'], e);
      if (action && (railAction(action) || run(action))) e.preventDefault();
      else if (!action) {
        const onPane = state.railPick?.pane !== null;
        extensionCommand(e, onPane ? ['anywhere', 'checkout', 'pane'] : ['anywhere', 'checkout']);
      }
      return;
    }
    // An open feature hears its own keys before Anywhere's: its j and k
    // move its lens, not the index under it, and its digits are the lenses.
    // Its expanded diagram hears its own over that. The note's field holds
    // every key but the stage switches and Escape.
    let feature: Place[] = ['anywhere', 'feature', 'card'];
    if (captureOpen()) feature = ['anywhere'];
    else if (workOpen() && expandedOpen()) feature = ['diagram', 'outline', 'anywhere'];
    else if (workOpen()) feature = ['outline', 'anywhere'];
    const places: Place[] =
      state.rail.stage === 'feature'
        ? feature
        : state.rail.stage === 'checkouts'
          ? ['anywhere', 'checkouts']
          : state.rail.stage === 'extension'
            ? ['anywhere', 'extension']
            : ['anywhere'];
    const action = actionFor(places, e);
    if (action && run(action)) e.preventDefault();
    else if (!action && $('layer').childElementCount === 0) {
      // A page's own action keys, then its commands, then the rest.
      const page = state.rail.stage === 'extension';
      const id = page ? extensionActionFor(e) : null;
      if (id) {
        e.preventDefault();
        call('extension_action', { action: id });
      } else {
        const checkouts = state.rail.stage === 'checkouts';
        extensionCommand(
          e,
          page ? ['view', 'anywhere'] : checkouts ? ['anywhere', 'checkout'] : ['anywhere'],
        );
      }
    }
  });
}
