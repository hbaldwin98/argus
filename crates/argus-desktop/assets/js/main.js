// The desktop window's page: it draws what Rust sends and reports what the
// person does. It decides nothing about the daemon — the model is the
// window's `session` — so each event replaces what the page held.
import { drawChrome, drawHeading, notice, reconnect } from './chrome.js';
import { $ } from './dom.js';
import { drawFeatures, listen as listenToFeatures } from './features.js';
import { listen as listenToInput } from './input.js';
import { screen } from './screen.js';
import { state } from './state.js';
import { connect } from './tauri.js';
function receive(event) {
    switch (event.type) {
        case 'status':
            state.status = event;
            drawChrome();
            break;
        case 'rail': {
            const before = state.rail.shown?.pane;
            state.rail = event;
            drawChrome();
            // A pane newly on screen takes the keys, however it got there: a
            // click on its row, or a shell started from a checkout's +.
            if (event.shown && event.shown.pane !== before)
                $('input').focus();
            break;
        }
        case 'frame':
            screen.frame(event);
            break;
        case 'clipboard':
            navigator.clipboard?.writeText(event.text).catch(() => { });
            break;
        case 'notice':
            notice(event.text);
            break;
        case 'features':
            state.features = event;
            drawFeatures();
            break;
    }
}
listenToInput();
listenToFeatures();
document.addEventListener('click', (e) => {
    if (state.menuOpen && e.target instanceof Node && !$('heading').contains(e.target)) {
        state.menuOpen = false;
        drawHeading();
    }
});
$('reconnect').addEventListener('click', reconnect);
drawChrome();
connect(receive);
