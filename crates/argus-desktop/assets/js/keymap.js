// The keys pressed outside a pane, looked up in the keymap Rust sends
// (`src/keymap.rs`) and run here. A pane's keys go to Rust whole, and its
// leader is dispatched there.
import { answerNo, checkoutsAction } from './checkouts.js';
import { startAction, startOpen } from './chrome.js';
import { $ } from './dom.js';
import { closeLayer, featureAction } from './features.js';
import { state } from './state.js';
import { call } from './tauri.js';
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
        if (answerNo())
            return true;
        if (layerOpen)
            closeLayer();
        return layerOpen;
    }
    // A menu or dialog over the stage holds the keys until it closes.
    if (layerOpen)
        return false;
    switch (action) {
        case 'workspace':
        case 'feature':
            call('show_features', { open: action === 'feature' });
            return true;
        case 'checkouts':
            call('show_checkouts', { open: true });
            return true;
        default:
            return state.rail.stage === 'checkouts' ? checkoutsAction(action) : featureAction(action);
    }
}
export function listen() {
    document.addEventListener('keydown', (e) => {
        // A text field takes its own keys, and the pane's text area sends its
        // keys to Rust.
        const typing = e.target instanceof Element && e.target.closest('input, textarea, [contenteditable]');
        if (typing)
            return;
        // An open + menu holds the keys until it closes.
        if (startOpen()) {
            const action = actionFor(['anywhere', 'start'], e);
            if (action && startAction(action))
                e.preventDefault();
            return;
        }
        const places = state.rail.stage === 'feature'
            ? ['anywhere', 'feature', 'card']
            : state.rail.stage === 'checkouts'
                ? ['anywhere', 'checkouts']
                : ['anywhere'];
        const action = actionFor(places, e);
        if (action && run(action))
            e.preventDefault();
    });
}
