// The keys pressed outside a pane, looked up in the keymap Rust sends
// (`src/keys/keymap.rs`) and run here by the layer `focus` finds has them.
// A pane's keys go to Rust whole, and its leader is dispatched there.
import { captureOpen, discardCapture } from '../capture.js';
import { answerNo, checkoutsAction } from '../checkouts/stage.js';
import { codeAction } from '../code/stage.js';
import { focusHistoryFilter, historyOpen } from '../history.js';
import { linkAction, linkOpen } from '../link.js';
import { reconcileAction, reconcileOpen } from '../reconcile.js';
import { reviewOpen } from '../review.js';
import { focusSessionsSearch, readerOpen, sessionsOpen } from '../agents/sessions_dialog.js';
import { focusTranscriptFind } from '../agents/transcript.js';
import { railAction, railPicked, startAction, startOpen } from '../chrome.js';
import { extensionDialogFocused } from '../extensions/dialogs.js';
import { $ } from '../kit/dom.js';
import { answerExtensionNo, extensionGesture, focusedExtension, pagedExtension, runExtensionCommand, } from '../extensions/views.js';
import { expandedOpen } from '../sequence.js';
import { featureAction } from '../features.js';
import { escapeFrom, takesFromPane, up } from './focus.js';
import { closeLayer } from '../kit/menu.js';
import { closePalette, openPalette } from './palette.js';
import { addProjectOpen, answerRemoveNo } from '../chrome/projects.js';
import { closeSettings, openSettings, settingsOpen } from '../settings/preferences.js';
import { closeSheet, openSheet, sheetOpen, toggleSheet } from './shortcuts.js';
import { state } from '../state.js';
import { call } from '../connection/tauri.js';
import { leaveWork, workAction, workOpen } from '../work.js';
/** `Chord::matches` again: the page decides before Rust hears the key. */
function matches(chord, e) {
    return (e.key === chord.key &&
        ([...chord.key].length === 1 || e.shiftKey === chord.shift) &&
        e.ctrlKey === chord.ctrl &&
        e.altKey === chord.alt &&
        e.metaKey === chord.meta);
}
/** What `e` does in `places`, leaving aside keys pressed after the leader.
 *  The places are asked in the order given, so one listed first may take a
 *  key from one after it: an open feature's digits are its lenses. */
export function actionFor(places, e) {
    for (const place of places) {
        const group = state.keymap.find((g) => g.place === place);
        for (const binding of group?.bindings ?? []) {
            if (binding.chords.some((chord) => !chord.leader && matches(chord, e)))
                return binding.action;
        }
    }
    return null;
}
/** The id of the action on the extension view out whose key `e` is: one
 *  character, typed with no modifier but its own shift. */
function extensionActionFor(e, view) {
    if (e.ctrlKey || e.altKey || e.metaKey)
        return null;
    const actions = view?.actions ?? [];
    return actions.find((action) => action.key !== null && action.key === e.key)?.id ?? null;
}
/** Runs the extension command whose key `e` is among those heard in
 *  `scopes`, saying whether there was one. The daemon refused any key the
 *  keymap holds, so none of these shadows one of the window's own. */
function extensionCommand(e, scopes) {
    const view = (focusedExtension() ?? pagedExtension())?.view ?? null;
    const command = (state.extensions?.commands ?? []).find((c) => scopes.includes(c.scope) &&
        (c.scope !== 'view' || c.view === view) &&
        c.chords.some((chord) => matches(chord, e)));
    if (!command)
        return false;
    e.preventDefault();
    runExtensionCommand(command.extension, command.command);
    return true;
}
/** Whether a menu or a question is over the stage. */
function layerOpen() {
    return $('layer').childElementCount > 0;
}
/** Runs `action`, saying whether there was anything for it to do. */
function run(action) {
    if (action === 'closeLayer')
        return escapeFrom(state.layers, handlers);
    // A menu or dialog over the stage holds the keys until it closes.
    if (layerOpen())
        return false;
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
        case 'code':
            call('show_code', { open: true });
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
        case 'extensionDock': {
            // The view with the keys, else the page; the palette's is the one
            // last used, which Rust knows.
            const view = focusedExtension() ?? pagedExtension();
            if (view)
                call('extension_mode', { panel: view.panel, page: action === 'extensionPage' });
            else
                call('command', { action });
            return true;
        }
        case 'views':
            call('toggle_views', {});
            return true;
        default:
            if (state.rail.stage === 'checkouts')
                return checkoutsAction(action);
            if (state.rail.stage === 'code')
                return codeAction(action);
            if (state.rail.stage === 'extension') {
                const moves = action === 'next' ||
                    action === 'previous' ||
                    action === 'first' ||
                    action === 'last' ||
                    action === 'choose';
                const page = pagedExtension();
                if (moves && page)
                    extensionGesture(page.panel, { on: 'key', action });
                return moves;
            }
            return workOpen() ? workAction(action) : featureAction(action);
    }
}
/** Opens the palette, the sheet or the settings, for a key here or one
 *  Rust heard in a pane: one over another would leave two holding the
 *  keys. */
export function runOver(action) {
    if (action === 'palette') {
        closeSheet();
        closeSettings();
        openPalette();
    }
    else if (action === 'shortcuts') {
        closePalette();
        closeSettings();
        toggleSheet();
    }
    else if (action === 'settings') {
        closePalette();
        closeSheet();
        openSettings();
    }
}
/** The keys a dialog does not answer, open over it. */
const NOT_THEIRS = new Set([
    'workspace',
    'feature',
    'checkouts',
    'compose',
    'console',
    'toggleConsole',
    'views',
    'palette',
    'settings',
]);
function sessionsAction(action) {
    call('sessions_action', { action, row: null });
}
/** An extension's view clicked: its own keys and its actions', then the
 *  commands heard there; any other key goes on to the layer under it. */
function extensionKeys(e, places) {
    const focused = focusedExtension();
    if (!focused)
        return false;
    const action = actionFor(places, e);
    const id = action ? null : extensionActionFor(e, focused);
    if (action === 'extensionPage' || action === 'extensionDock')
        run(action);
    else if (action)
        extensionGesture(focused.panel, { on: 'key', action });
    else if (id)
        extensionGesture(focused.panel, { on: 'act', action: id });
    if (action || id) {
        e.preventDefault();
        return true;
    }
    return extensionCommand(e, ['view', 'anywhere']);
}
function railKeys(e, places) {
    const action = actionFor(places, e);
    if (action && (railAction(action) || run(action)))
        e.preventDefault();
    else if (!action) {
        const onPane = state.railPick?.pane !== null;
        extensionCommand(e, onPane ? ['anywhere', 'checkout', 'pane'] : ['anywhere', 'checkout']);
    }
    return true;
}
/** The stage's keys, the menu layer being down. */
function stageKeys(e, places) {
    const action = actionFor(places, e);
    if (action && run(action))
        e.preventDefault();
    else if (!action) {
        // A page's own action keys, then its commands, then the rest.
        const page = pagedExtension();
        const id = page ? extensionActionFor(e, page) : null;
        if (page && id) {
            e.preventDefault();
            extensionGesture(page.panel, { on: 'act', action: id });
        }
        else {
            const checkouts = state.rail.stage === 'checkouts';
            extensionCommand(e, page ? ['view', 'anywhere'] : checkouts ? ['anywhere', 'checkout'] : ['anywhere']);
        }
    }
    return true;
}
function capturing() {
    return state.rail.stage === 'feature' && captureOpen();
}
/** What each layer does while it has the keys; `src/keys/keymap.rs` orders
 *  them and says what places each hears. */
const handlers = {
    // The pane's text area sends its keys to Rust.
    field: {
        open: (target) => target instanceof Element && target.closest('input, textarea, [contenteditable]') !== null,
    },
    extensionDialog: { open: extensionDialogFocused },
    // The settings answer their own keys, Escape among them.
    settings: { open: settingsOpen },
    sheet: {
        open: sheetOpen,
        keys: (e, places) => {
            const action = actionFor(places, e);
            if (action === 'closeLayer' || action === 'shortcuts') {
                e.preventDefault();
                closeSheet();
            }
            return true;
        },
    },
    addProject: { open: addProjectOpen },
    link: { open: linkOpen, sink: linkAction },
    reconcile: { open: reconcileOpen, sink: reconcileAction },
    review: { open: reviewOpen, sink: (action) => call('review_action', { action }) },
    history: {
        open: historyOpen,
        filter: focusHistoryFilter,
        sink: (action) => call('history_action', { action, row: null }),
    },
    transcript: { open: readerOpen, filter: focusTranscriptFind, sink: sessionsAction },
    sessions: { open: sessionsOpen, filter: focusSessionsSearch, sink: sessionsAction },
    start: {
        open: startOpen,
        keys: (e, places) => {
            const action = actionFor(places, e);
            if (action && startAction(action))
                e.preventDefault();
            return true;
        },
    },
    extension: { open: () => focusedExtension() !== null && !layerOpen(), keys: extensionKeys },
    // A picked row holds the keys until a click elsewhere.
    rail: { open: railPicked, keys: railKeys },
    menu: {
        open: layerOpen,
        keys: (e, places) => {
            if (actionFor(places, e) === 'closeLayer' && run('closeLayer'))
                e.preventDefault();
            return true;
        },
        escape: () => {
            // A question the stage asked is answered no, not only put away.
            if (answerNo() || answerRemoveNo() || answerExtensionNo())
                return true;
            if (!layerOpen())
                return false;
            closeLayer();
            return true;
        },
    },
    capture: {
        open: capturing,
        keys: stageKeys,
        escape: () => {
            if (!capturing())
                return false;
            discardCapture();
            return true;
        },
    },
    diagram: { open: () => workOpen() && expandedOpen(), keys: stageKeys },
    // Back to the index, the diagram or a form being written put away first.
    outline: {
        open: workOpen,
        keys: stageKeys,
        escape: () => {
            if (!workOpen())
                return false;
            leaveWork();
            return true;
        },
    },
    features: { open: () => state.rail.stage === 'feature', keys: stageKeys },
    checkouts: { open: () => state.rail.stage === 'checkouts', keys: stageKeys },
    code: {
        open: () => state.rail.stage === 'code',
        keys: stageKeys,
        // Back to the review it was opened from, or to the stage before.
        escape: () => state.rail.stage === 'code' && codeAction('closeLayer'),
    },
    extensionPage: { open: () => state.rail.stage === 'extension', keys: stageKeys },
    stage: { open: () => true, keys: stageKeys },
};
/** What the layer with the keys does with `e`, saying whether it held it.
 *  A dialog holds every key while open, all but the sheet's, which lists
 *  theirs, and the window's own. */
function hear(layer, e) {
    const handler = handlers[layer.layer];
    if (layer.places === null)
        return true;
    if (handler.keys)
        return handler.keys(e, layer.places);
    const action = actionFor(layer.places, e);
    if (action === 'shortcuts') {
        e.preventDefault();
        openSheet();
    }
    else if (action && !NOT_THEIRS.has(action)) {
        e.preventDefault();
        if (action === 'filter' && handler.filter)
            handler.filter();
        else
            handler.sink?.(action);
    }
    return true;
}
/** Whether a dialog that takes the keys from a pane is open. */
export function dialogTakesKeys() {
    return takesFromPane(state.layers, handlers, document.activeElement);
}
export function listen() {
    // The window's keys come first, while the event is still on its way
    // down: a pane's text area, a field or a dialog never hears them.
    document.addEventListener('keydown', (e) => {
        const action = actionFor(['window'], e);
        if (!action)
            return;
        e.preventDefault();
        e.stopPropagation();
        call('command', { action });
    }, { capture: true });
    document.addEventListener('keydown', (e) => {
        for (const layer of up(state.layers, handlers, e.target)) {
            if (hear(layer, e))
                return;
        }
    });
}
