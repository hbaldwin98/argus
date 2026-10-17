// The pane's keys, text and pastes, taken by a hidden text area so input
// methods and the clipboard work as they do anywhere else; the mouse over
// each screen; and the window's focus, which says whether this window's
// view is the person's.
import { keyedScreen } from './console.js';
import { $ } from '../kit/dom.js';
import { actionFor } from '../keymap.js';
import { screen, screens, watchScreens } from './screen.js';
import { call } from '../connection/tauri.js';
const MODIFIERS = new Set(['Shift', 'Control', 'Alt', 'Meta', 'AltGraph', 'CapsLock']);
const MAC = /Macintosh|Mac OS X/.test(navigator.userAgent);
function drawCursors() {
    for (const s of screens)
        s.drawCursor();
}
function keyed(s) {
    return s === keyedScreen();
}
export function listen() {
    const input = $('input');
    if (!(input instanceof HTMLTextAreaElement))
        throw new Error('#input is not a text area');
    input.addEventListener('keydown', (e) => {
        // WebKit reports an input method's keys as 229 without isComposing.
        if (e.isComposing || e.keyCode === 229)
            return;
        // The command key's chords are the window's (quit, close), but for
        // those the keymap gives the pane: paste.
        if (MAC && e.metaKey && !actionFor(['pane'], e))
            return;
        if (MODIFIERS.has(e.key))
            return;
        for (const s of screens)
            s.select(null);
        // Every other key goes to Rust, which dispatches it from the keymap:
        // the leader and the paste chord are the window's, the rest the
        // pane's. No webview accelerator (F5, Ctrl+F, Ctrl+R) fires behind the
        // person's back.
        e.preventDefault();
        call('key', {
            key: {
                key: e.key,
                code: e.code,
                ctrl: e.ctrlKey,
                alt: e.altKey,
                shift: e.shiftKey,
                meta: e.metaKey,
            },
        });
    });
    input.addEventListener('compositionend', (e) => {
        if (e.data)
            call('text', { text: e.data });
        input.value = '';
    });
    input.addEventListener('input', (e) => {
        if (!(e instanceof InputEvent) || !e.isComposing)
            input.value = '';
    });
    input.addEventListener('paste', (e) => {
        e.preventDefault();
        const text = e.clipboardData?.getData('text/plain');
        if (text)
            call('paste', { text });
    });
    input.addEventListener('focus', drawCursors);
    input.addEventListener('blur', drawCursors);
    // A drag selects, and letting go copies, as in the terminal client. A
    // child that asked for the mouse gets its presses, drags and moves
    // instead, as Rust encodes them for its mode; Shift-drag still selects.
    // Rust sends these to the pane with the keys, which a press on another
    // screen — the Workspace's, or a side of the console — hands over first.
    let on = screen;
    let anchor = null;
    let moved = false;
    let held = null;
    let last = null;
    const report = (gesture, button, at) => {
        last = at;
        call('mouse', { gesture, button, at });
    };
    const moving = (at) => !last || last[0] !== at[0] || last[1] !== at[1];
    const attach = (s) => {
        s.terminal.addEventListener('mousedown', (e) => {
            e.preventDefault();
            input.focus();
            const pane = s.pane();
            if (!keyed(s) && pane !== null) {
                const side = s.side();
                if (s === screen)
                    call('select', { pane });
                else if (typeof side === 'object' && side !== null)
                    call('focus_view', { pane });
                else
                    call('console', { gesture: { on: 'select', pane } });
            }
            for (const other of screens)
                if (other !== s)
                    other.select(null);
            on = s;
            const at = s.cellAt(e.clientX, e.clientY);
            if (s.takesMouse() && !e.shiftKey) {
                held = e.button;
                report('down', e.button, at);
                return;
            }
            if (e.button !== 0)
                return;
            anchor = at;
            moved = false;
            s.select(null);
        });
        s.terminal.addEventListener('contextmenu', (e) => e.preventDefault());
        s.terminal.addEventListener('mousemove', (e) => {
            if (anchor || held !== null || !s.takesMouse() || !keyed(s))
                return;
            const at = s.cellAt(e.clientX, e.clientY);
            if (moving(at))
                report('move', 0, at);
        });
        // Rust decides what a notch is to the pane, and scrolls the one under
        // the pointer whichever side has the keys. A notch of a physical wheel
        // is 100 pixels or 3 lines; a touchpad's small deltas add up to one.
        let wheel = 0;
        s.terminal.addEventListener('wheel', (e) => {
            e.preventDefault();
            const pane = s.pane();
            if (pane === null)
                return;
            wheel += e.deltaMode === WheelEvent.DOM_DELTA_LINE ? e.deltaY / 3 : e.deltaY / 100;
            const notches = Math.trunc(wheel);
            if (!notches)
                return;
            wheel -= notches;
            call('wheel', { pane, notches, at: s.cellAt(e.clientX, e.clientY) });
        }, { passive: false });
    };
    watchScreens(attach);
    window.addEventListener('mousemove', (e) => {
        if (held !== null) {
            const at = on.cellAt(e.clientX, e.clientY);
            if (moving(at))
                report('drag', held, at);
            return;
        }
        if (!anchor)
            return;
        const head = on.cellAt(e.clientX, e.clientY);
        moved ||= head[0] !== anchor[0] || head[1] !== anchor[1];
        if (moved)
            on.select({ anchor, head });
    });
    window.addEventListener('mouseup', (e) => {
        if (held !== null) {
            if (e.button === held) {
                report('up', held, on.cellAt(e.clientX, e.clientY));
                held = null;
            }
            return;
        }
        if (!anchor)
            return;
        if (moved)
            call('copy', { from: anchor, to: on.cellAt(e.clientX, e.clientY) });
        anchor = null;
    });
    window.addEventListener('focus', () => {
        call('focus', { focused: true });
        drawCursors();
    });
    window.addEventListener('blur', () => {
        call('focus', { focused: false });
        drawCursors();
    });
}
