// The keys pressed outside a pane, looked up in the keymap Rust sends
// (`src/keymap.rs`) and run here. A pane's keys go to Rust whole, and its
// leader is dispatched there.
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
        default:
            return featureAction(action);
    }
}
export function listen() {
    document.addEventListener('keydown', (e) => {
        // A text field takes its own keys, and the pane's text area sends its
        // keys to Rust.
        const typing = e.target instanceof Element && e.target.closest('input, textarea, [contenteditable]');
        if (typing)
            return;
        const places = state.rail.stage === 'feature' ? ['anywhere', 'feature', 'card'] : ['anywhere'];
        const action = actionFor(places, e);
        if (action && run(action))
            e.preventDefault();
    });
}
