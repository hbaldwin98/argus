// The keys pressed outside a pane, looked up in the keymap Rust sends
// (`src/keymap.rs`) and run here. A pane's keys go to Rust whole, and its
// leader is dispatched there.
import { captureOpen, discardCapture } from './capture.js';
import { answerNo, checkoutsAction } from './checkouts.js';
import { focusHistoryFilter, historyOpen } from './history.js';
import { reviewOpen } from './review.js';
import { focusSessionsSearch, readerOpen, sessionsOpen } from './sessions.js';
import { focusTranscriptFind } from './transcript.js';
import { railAction, railPicked, startAction, startOpen } from './chrome.js';
import { $ } from './dom.js';
import { closeLayer, featureAction } from './features.js';
import { closePalette, openPalette } from './palette.js';
import { addProjectOpen, answerRemoveNo } from './projects.js';
import { closeSettings, openSettings, settingsOpen } from './settings.js';
import { closeSheet, openSheet, sheetOpen, toggleSheet } from './shortcuts.js';
import { state } from './state.js';
import { call } from './tauri.js';
import { leaveWork, workAction, workOpen } from './work.js';
/** `Chord::matches` again: the page decides before Rust hears the key. */
function matches(chord, e) {
    return (e.key === chord.key &&
        ([...chord.key].length === 1 || e.shiftKey === chord.shift) &&
        e.ctrlKey === chord.ctrl &&
        e.altKey === chord.alt &&
        e.metaKey === chord.meta);
}
/** What `e` does in `places`, leaving aside keys pressed after the leader. */
export function actionFor(places, e) {
    for (const group of state.keymap) {
        if (!places.includes(group.place))
            continue;
        for (const binding of group.bindings) {
            if (binding.chords.some((chord) => !chord.leader && matches(chord, e)))
                return binding.action;
        }
    }
    return null;
}
/** Runs `action`, saying whether there was anything for it to do. */
function run(action) {
    const layerOpen = $('layer').childElementCount > 0;
    if (action === 'closeLayer') {
        // A question the stage asked is answered no, not only put away.
        if (answerNo() || answerRemoveNo())
            return true;
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
    if (layerOpen)
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
        case 'views':
            call('toggle_views', {});
            return true;
        default:
            if (state.rail.stage === 'checkouts')
                return checkoutsAction(action);
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
function notTheirs(action) {
    return (action === 'workspace' ||
        action === 'feature' ||
        action === 'checkouts' ||
        action === 'compose' ||
        action === 'console' ||
        action === 'toggleConsole' ||
        action === 'views' ||
        action === 'palette' ||
        action === 'settings');
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
        // A text field takes its own keys, and the pane's text area sends its
        // keys to Rust.
        const typing = e.target instanceof Element && e.target.closest('input, textarea, [contenteditable]');
        if (typing)
            return;
        // The settings answer their own keys, Escape among them.
        if (settingsOpen())
            return;
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
        if (addProjectOpen())
            return;
        // The review, history and sessions dialogs hold the keys while open,
        // all but the sheet's, which lists theirs; Rust runs the rest.
        if (reviewOpen()) {
            const action = actionFor(['anywhere', 'review'], e);
            if (action === 'shortcuts') {
                e.preventDefault();
                openSheet();
            }
            else if (action && !notTheirs(action)) {
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
            }
            else if (action && !notTheirs(action)) {
                e.preventDefault();
                if (action === 'filter')
                    focusHistoryFilter();
                else
                    call('history_action', { action, row: null });
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
            }
            else if (action && !notTheirs(action)) {
                e.preventDefault();
                if (action === 'filter') {
                    if (reading)
                        focusTranscriptFind();
                    else
                        focusSessionsSearch();
                }
                else
                    call('sessions_action', { action, row: null });
            }
            return;
        }
        // An open + menu holds the keys until it closes.
        if (startOpen()) {
            const action = actionFor(['anywhere', 'start'], e);
            if (action && startAction(action))
                e.preventDefault();
            return;
        }
        // A row picked in the rail holds the keys until a click elsewhere.
        if (railPicked()) {
            const action = actionFor(['anywhere', 'rail'], e);
            if (action && (railAction(action) || run(action)))
                e.preventDefault();
            return;
        }
        // An open feature hears its own keys: its j and k move its outline,
        // not the index under it. The note's field holds every key but the
        // stage switches and Escape.
        let feature = ['anywhere', 'feature', 'card'];
        if (captureOpen())
            feature = ['anywhere'];
        else if (workOpen())
            feature = ['anywhere', 'outline'];
        const places = state.rail.stage === 'feature'
            ? feature
            : state.rail.stage === 'checkouts'
                ? ['anywhere', 'checkouts']
                : ['anywhere'];
        const action = actionFor(places, e);
        if (action && run(action))
            e.preventDefault();
    });
}
