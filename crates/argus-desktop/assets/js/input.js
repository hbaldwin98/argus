// The pane's keys, text and pastes, taken by a hidden text area so input
// methods and the clipboard work as they do anywhere else, and the
// window's focus, which says whether this window's view is the person's.
import { $ } from './dom.js';
import { screen } from './screen.js';
import { call } from './tauri.js';
const MODIFIERS = new Set(['Shift', 'Control', 'Alt', 'Meta', 'AltGraph', 'CapsLock']);
const MAC = /Macintosh|Mac OS X/.test(navigator.userAgent);
function isPasteChord(e) {
    const key = e.key.toLowerCase();
    return MAC ? e.metaKey && key === 'v' : e.ctrlKey && e.shiftKey && key === 'v';
}
export function listen() {
    const input = $('input');
    if (!(input instanceof HTMLTextAreaElement))
        throw new Error('#input is not a text area');
    input.addEventListener('keydown', (e) => {
        // WebKit reports an input method's keys as 229 without isComposing.
        if (e.isComposing || e.keyCode === 229)
            return;
        // Left to the browser so its paste event fires.
        if (isPasteChord(e))
            return;
        // The command key's chords are the window's (quit, close).
        if (MAC && e.metaKey)
            return;
        if (MODIFIERS.has(e.key))
            return;
        // Every other key goes to the pane, so no webview accelerator (F5,
        // Ctrl+F, Ctrl+R) fires behind the person's back.
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
    input.addEventListener('focus', () => screen.drawCursor());
    input.addEventListener('blur', () => screen.drawCursor());
    $('terminal').addEventListener('mousedown', (e) => {
        e.preventDefault();
        input.focus();
    });
    window.addEventListener('focus', () => {
        call('focus', { focused: true });
        screen.drawCursor();
    });
    window.addEventListener('blur', () => {
        call('focus', { focused: false });
        screen.drawCursor();
    });
    // A pane appearing after the empty stage needs its room reported.
    new MutationObserver(() => screen.report()).observe($('terminal'), {
        attributes: true,
        attributeFilter: ['hidden'],
    });
}
