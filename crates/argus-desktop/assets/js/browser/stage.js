// The Browsers stage: every browser agents' panes run, each a live tile,
// or the one chosen filling the stage with its controls. Rust says which
// browsers there are and which is chosen (`src/browser/mod.rs`); their
// pictures come apart, many a second, and are painted onto each one's
// canvas as they come. The page's CSP allows no `data:` image, so a
// picture is decoded to a bitmap and drawn rather than set as a source.
import { $, el } from '../kit/dom.js';
import { actionFor } from '../keys/keymap.js';
import { call } from '../connection/tauri.js';
import { address, keyInput, within } from './input.js';
let page = { browsers: [], chosen: null };
const shots = new Map();
/** Each browser's canvas, kept across redraws so a tile does not go blank
 *  between pictures. */
const canvases = new Map();
/** The newest picture each canvas has started decoding, so one that
 *  finishes after a newer one is dropped. */
const painting = new Map();
let urlField = null;
export function browsersHeard(event) {
    page = event;
    const open = new Set(event.browsers.map((b) => b.pane));
    for (const pane of canvases.keys()) {
        if (!open.has(pane)) {
            canvases.delete(pane);
            shots.delete(pane);
        }
    }
    drawBrowsers();
}
export function browserShot(shot) {
    shots.set(shot.pane, shot);
    paint(shot.pane);
}
/** The strip's title on this stage: the chosen browser's agent. */
export function browsersTitle() {
    const chosen = chosenBrowser();
    return chosen ? chosen.agent : 'Every agent';
}
function chosenBrowser() {
    return page.browsers.find((b) => b.pane === page.chosen) ?? null;
}
export function drawBrowsers() {
    const stage = $('bstage');
    if (stage.hidden)
        return;
    const chosen = chosenBrowser();
    if (chosen) {
        stage.replaceChildren(chosenView(chosen));
    }
    else if (page.browsers.length === 0) {
        stage.replaceChildren(el('div', { class: 'empty' }, el('div', { class: 'empty-body' }, el('p', null, 'No agent has a browser open.'), el('p', { class: 'fg-muted' }, 'An agent opens one with ', el('code', null, 'argus-hook browser'), ' and drives it with its own browser tool; it shows here as it works.'))));
    }
    else {
        stage.replaceChildren(el('div', { class: 'bgrid' }, page.browsers.map(tile)));
    }
    for (const browser of page.browsers)
        paint(browser.pane);
}
/** Sends the person's hand to `pane`'s browser. */
function send(pane, input) {
    call('browser_input', { pane, input });
}
/** A browser's canvas, made once. It answers the mouse and keys only while
 *  its browser fills the stage; as a tile, a press opens it instead. */
function canvasFor(pane) {
    const held = canvases.get(pane);
    if (held)
        return held;
    const canvas = el('canvas', { class: 'bscreen' });
    const chosen = () => page.chosen === pane;
    canvas.addEventListener('mousedown', (e) => {
        if (!chosen())
            return;
        canvas.focus();
        const at = point(pane, canvas, e);
        if (at)
            send(pane, { kind: 'click', ...at });
    });
    canvas.addEventListener('wheel', (e) => {
        if (!chosen())
            return;
        e.preventDefault();
        const at = point(pane, canvas, e);
        if (at)
            send(pane, { kind: 'wheel', ...at, dx: e.deltaX, dy: e.deltaY });
    });
    canvas.addEventListener('keydown', (e) => {
        const input = chosen() ? keyInput(e) : null;
        if (!input)
            return;
        // The browser's, not the window's: a digit typed into a form is not
        // a stage key.
        e.preventDefault();
        e.stopPropagation();
        send(pane, input);
    });
    canvases.set(pane, canvas);
    return canvas;
}
/** Where the page is, said once: its title, and its address under it. */
function where(browser) {
    const title = browser.title && browser.title !== browser.url ? browser.title : '';
    return el('span', { class: 'bwhere' }, title ? el('span', { class: 'btitle' }, title) : null, el('span', { class: 'burl mono fg-muted' }, browser.url || 'about:blank'));
}
function tile(browser) {
    const canvas = canvasFor(browser.pane);
    canvas.removeAttribute('tabindex');
    return el('button', {
        class: 'btile',
        type: 'button',
        title: `Open ${browser.agent}'s browser`,
        onclick: () => call('show_browsers', { pane: browser.pane }),
    }, el('span', { class: 'btile-head' }, el('span', { class: 'bagent' }, browser.agent), where(browser)), el('span', { class: 'btile-screen' }, canvas));
}
function chosenView(browser) {
    const pane = browser.pane;
    const canvas = canvasFor(pane);
    canvas.tabIndex = 0;
    canvas.setAttribute('aria-label', `${browser.agent}'s browser`);
    const field = urlFieldFor(browser.url);
    return el('div', { class: 'bchosen' }, el('div', { class: 'bbar' }, button('All browsers', 'Every agent’s browser at once', () => call('show_browsers', { pane: null })), button('←', 'Back', () => send(pane, { kind: 'back' })), button('→', 'Forward', () => send(pane, { kind: 'forward' })), button('⟳', 'Reload', () => send(pane, { kind: 'reload' })), field, button('Close', 'Close this browser; the agent’s next request opens another', () => call('close_browser', { pane }))), el('div', { class: 'bchosen-screen' }, canvas));
}
function button(label, title, onclick) {
    return el('button', { class: 'btn', type: 'button', title, onclick }, label);
}
/** The address field, made once so typing in it survives the page
 *  changing under it; it shows the chosen page's address unless the
 *  person is mid-edit. */
function urlFieldFor(url) {
    if (!urlField) {
        const field = el('input', {
            class: 'field burl-field mono',
            'aria-label': 'Address',
            spellcheck: 'false',
        });
        field.addEventListener('keydown', (e) => {
            // Its keys are its own, never the window's.
            e.stopPropagation();
            const chosen = chosenBrowser();
            const action = actionFor(['address'], e);
            if (!chosen || !action)
                return;
            e.preventDefault();
            if (action === 'choose')
                send(chosen.pane, { kind: 'navigate', url: address(field.value) });
            else
                field.value = chosen.url;
            field.blur();
        });
        urlField = field;
    }
    if (document.activeElement !== urlField)
        urlField.value = url;
    return urlField;
}
/** The point under the mouse in the page's CSS pixels, which the browser
 *  takes its input in. */
function point(pane, canvas, e) {
    const shot = shots.get(pane);
    const box = canvas.getBoundingClientRect();
    if (!shot || box.width === 0 || box.height === 0)
        return null;
    return within(box, shot, e.clientX, e.clientY);
}
function paint(pane) {
    const shot = shots.get(pane);
    const canvas = canvases.get(pane);
    if (!shot || !canvas || !canvas.isConnected)
        return;
    const ticket = (painting.get(pane) ?? 0) + 1;
    painting.set(pane, ticket);
    const bytes = Uint8Array.from(atob(shot.jpeg), (c) => c.charCodeAt(0));
    createImageBitmap(new Blob([bytes], { type: 'image/jpeg' }))
        .then((bitmap) => {
        if (painting.get(pane) !== ticket)
            return bitmap.close();
        if (canvas.width !== bitmap.width)
            canvas.width = bitmap.width;
        if (canvas.height !== bitmap.height)
            canvas.height = bitmap.height;
        canvas.getContext('2d')?.drawImage(bitmap, 0, 0);
        bitmap.close();
    })
        .catch(() => { });
}
