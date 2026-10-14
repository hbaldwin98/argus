// The desktop window's page: it draws what Rust sends and reports what the
// person does. It decides nothing about the daemon — the model is the
// window's `session` — so each event replaces what the page held.
import { drawChrome, drawHeading, notice, reconnect } from './chrome.js';
import { $ } from './dom.js';
import { drawFeatures, listen as listenToFeatures } from './features.js';
import { listen as listenToInput } from './input.js';
import { listen as listenToKeys } from './keymap.js';
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
            const before = state.rail;
            state.rail = event;
            drawChrome();
            document.body.classList.toggle('filled', event.filled);
            // A pane newly on screen takes the keys, however it got there: a
            // click on its row, or a shell started from a checkout's +. So does
            // the pane coming back with the Workspace, and leaving it for the
            // Feature stage lets the keys go to the stage.
            const shown = event.shown && event.shown.pane !== before.shown?.pane;
            if (event.stage !== before.stage && event.stage === 'feature')
                $('input').blur();
            else if (shown || (event.shown && event.stage !== before.stage))
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
        case 'keymap':
            state.keymap = event.groups;
            break;
    }
}
listenToInput();
listenToKeys();
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
