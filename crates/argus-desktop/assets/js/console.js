// The console: a tab per console the daemon holds, its buttons, the sash
// it is resized by while docked, and its sides — a screen of its own on
// the first, under the Workspace's, and the second beside it while it is
// split, each headed by its console. Rust decides where the console is,
// which panes its sides draw and which has the keys (`src/console.rs`);
// this draws that, puts the keys' text area in the screen that has them,
// and reports what was pressed.
import { $, el, icon } from './dom.js';
import { chordsFor, presses } from './keys.js';
import { contextMenu } from './menu.js';
import { renameDialog } from './rename.js';
import { beside, consoleScreen, screen } from './screen.js';
import { state } from './state.js';
import { call } from './tauri.js';
function gesture(g) {
    call('console', { gesture: g });
}
/** A button's title with its key from the keymap, as the nav's are. */
function titled(words, action) {
    const place = action === 'console' || action === 'toggleConsole' ? 'anywhere' : 'pane';
    const keys = chordsFor(action, place)
        .map((chord) => presses(chord).join(' '))
        .join(', ');
    return keys ? `${words} (${keys})` : words;
}
/** What a console's tab or split side offers; `side` adds taking it out
 *  of the split. */
function tabMenu(tab, side) {
    const others = state.console?.tabs.filter((t) => t.pane !== tab.pane) ?? [];
    return [
        { heading: tab.title },
        { label: 'Rename…', run: () => renameDialog(tab.pane, tab.title) },
        side && {
            label: 'Take out of the split',
            run: () => gesture({ on: 'closeSide', pane: tab.pane }),
        },
        'sep',
        { label: 'Close', danger: true, run: () => gesture({ on: 'close', pane: tab.pane }) },
        others.length > 0 && {
            label: 'Close the others',
            danger: true,
            run: () => {
                for (const t of others)
                    gesture({ on: 'close', pane: t.pane });
            },
        },
    ].filter((item) => !!item);
}
const PLACES = ['hidden', 'docked', 'tab', 'fullscreen'];
export function drawConsole() {
    const page = state.console;
    const place = page?.place ?? 'hidden';
    for (const p of PLACES)
        document.body.classList.toggle(`console-${p}`, place === p);
    $('console').hidden = place === 'hidden';
    $('console-sash').hidden = place !== 'docked';
    if (page)
        $('console').style.height = place === 'docked' ? `${page.height}px` : '';
    drawHead();
    placeTerminal();
}
function drawHead() {
    const page = state.console;
    if (!page) {
        $('console-head').replaceChildren();
        return;
    }
    const docked = page.place === 'docked';
    const full = page.place === 'fullscreen';
    const split = page.beside !== null;
    const onScreen = (pane) => pane === page.showing || pane === page.beside;
    $('console-head').replaceChildren(el('span', { class: 'label' }, 'Console'), el('div', { class: 'ctabs', role: 'tablist' }, page.tabs.map((tab) => el('div', {
        class: `ctab${tab.on ? ' on' : ''}${onScreen(tab.pane) ? ' live' : ''}`,
        oncontextmenu: contextMenu(tab.title, () => tabMenu(tab, false)),
    }, el('button', {
        class: 'ctab-pick',
        role: 'tab',
        'aria-selected': String(tab.on),
        title: `shell, ${tab.status}`,
        onclick: () => gesture({ on: 'select', pane: tab.pane }),
    }, el('span', { class: 'glyph sh' }, '›'), el('span', { class: 'ttl' }, tab.title), el('span', { class: `sdot ${tab.status}`, 'aria-hidden': 'true' })), el('button', {
        class: 'icon x',
        title: 'Close',
        'aria-label': `Close ${tab.title}`,
        onclick: () => gesture({ on: 'close', pane: tab.pane }),
    }, icon('close'))))), page.canSpawn
        ? el('button', {
            class: 'icon',
            title: 'New console',
            'aria-label': 'New console',
            onclick: () => gesture({ on: 'spawn' }),
        }, icon('plus'))
        : '', el('span', { class: 'grow' }), el('button', {
        class: `icon${split ? ' hot' : ''}`,
        title: titled(split ? 'Put the split away' : 'Split', 'splitConsole'),
        'aria-label': 'Split',
        'aria-pressed': String(split),
        onclick: () => gesture({ on: 'split' }),
    }, icon('split')), el('span', { class: 'csep' }), docked
        ? el('button', {
            class: 'icon',
            title: titled('Open in the Console stage', 'console'),
            'aria-label': 'Open in the Console stage',
            onclick: () => gesture({ on: 'tab' }),
        }, icon('up'))
        : el('button', {
            class: 'icon',
            title: 'Dock under the stage',
            'aria-label': 'Dock under the stage',
            onclick: () => gesture({ on: 'dock' }),
        }, icon('down')), el('button', {
        class: `icon${full ? ' hot' : ''}`,
        title: titled(full ? 'Leave fullscreen' : 'Fullscreen', 'consoleFullscreen'),
        'aria-label': 'Fullscreen',
        'aria-pressed': String(full),
        onclick: () => gesture({ on: 'fullscreen' }),
    }, icon('expand')), el('button', {
        class: 'icon',
        title: titled('Hide the console', 'toggleConsole'),
        'aria-label': 'Hide the console',
        onclick: () => gesture({ on: 'hide' }),
    }, icon('close')));
}
/** A split side's header: a press gives that side the keys, its close
 *  takes it out of the split and leaves its console a tab. */
function drawSideHead(id, pane) {
    const page = state.console;
    const head = $(id);
    const tab = page?.tabs.find((t) => t.pane === pane);
    head.hidden = !page || page.beside === null || !tab;
    if (head.hidden || !page || !tab) {
        head.replaceChildren();
        return;
    }
    head.classList.toggle('on', page.typing === tab.pane);
    head.replaceChildren(el('button', {
        class: 'thead-pick',
        title: page.typing === tab.pane ? tab.title : `Type in ${tab.title}`,
        onclick: () => gesture({ on: 'select', pane: tab.pane }),
        oncontextmenu: contextMenu(tab.title, () => tabMenu(tab, true)),
    }, el('span', { class: 'glyph sh' }, '›'), el('span', { class: 'ttl' }, tab.title), el('span', { class: `sdot ${tab.status}`, 'aria-hidden': 'true' })), el('button', {
        class: 'icon x',
        title: 'Take out of the split',
        'aria-label': `Take ${tab.title} out of the split`,
        onclick: () => gesture({ on: 'closeSide', pane: tab.pane }),
    }, icon('close')));
}
/** The screen with the keys: a console side the console says is typed
 *  in, else the Workspace's. */
export function keyedScreen() {
    const page = state.console;
    if (page?.typing == null)
        return screen;
    if (page.typing === page.beside)
        return beside;
    return page.typing === page.showing ? consoleScreen : screen;
}
/** The console's sides show while it draws a console, the second only
 *  while it is split, and the keys' text area goes into whichever screen
 *  has them, keeping its focus across the move. */
export function placeTerminal() {
    const page = state.console;
    const inConsole = page?.showing != null && page.place !== 'hidden';
    const split = inConsole && page?.beside != null;
    $('console-first').hidden = !inConsole;
    $('console-second').hidden = !split;
    drawSideHead('console-first-head', page?.showing ?? null);
    drawSideHead('console-second-head', page?.beside ?? null);
    consoleScreen.expect(inConsole ? (page?.showing ?? null) : null);
    beside.expect(split ? (page?.beside ?? null) : null);
    const keyed = keyedScreen();
    for (const s of [screen, consoleScreen, beside])
        s.key(s === keyed);
    const input = $('input');
    if (input.parentElement !== keyed.terminal) {
        const hadKeys = document.activeElement === input;
        keyed.terminal.append(input);
        if (hadKeys)
            input.focus();
    }
    const empty = $('console-empty');
    empty.hidden = inConsole;
    if (inConsole || !page)
        return;
    empty.replaceChildren(page.tabs.length === 0
        ? el('div', { class: 'empty-body' }, el('div', { class: 'fg-muted' }, 'No consoles yet: a shell in your home directory, in no checkout.'), page.canSpawn &&
            el('button', { class: 'btn', onclick: () => gesture({ on: 'spawn' }) }, 'Start a console'))
        : el('button', {
            class: 'btn',
            onclick: () => {
                const on = page.tabs.find((tab) => tab.on) ?? page.tabs[0];
                if (on)
                    gesture({ on: 'select', pane: on.pane });
            },
        }, 'Type in the console'));
}
/** The sash: the console follows the pointer while it is held, and its
 *  height is reported once it is let go, for Rust to keep and send back. */
export function listen() {
    const sash = $('console-sash');
    sash.addEventListener('mousedown', (e) => {
        if (e.button !== 0)
            return;
        e.preventDefault();
        const stage = $('console').parentElement;
        if (!stage)
            return;
        const bottom = stage.getBoundingClientRect().bottom;
        let height = $('console').offsetHeight;
        const move = (m) => {
            height = Math.round(bottom - m.clientY);
            $('console').style.height = `${height}px`;
        };
        const up = () => {
            window.removeEventListener('mousemove', move);
            window.removeEventListener('mouseup', up);
            // Drawn from what Rust keeps, which may hold it to its bounds and
            // so send nothing new.
            drawConsole();
            gesture({ on: 'height', height: Math.max(0, height) });
        };
        window.addEventListener('mousemove', move);
        window.addEventListener('mouseup', up);
    });
}
