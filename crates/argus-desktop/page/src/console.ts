// The console: a tab per console the daemon holds, its buttons, the sash
// it is resized by while docked, and where the window's one canvas is. Rust
// decides where the console is and which pane the canvas draws
// (`src/console.rs`); this draws that and reports what was pressed.

import type { ConsoleGesture } from './bindings/ConsoleGesture';
import { $, el, icon } from './dom.js';
import { chordsFor, presses } from './keys.js';
import { state } from './state.js';
import { call } from './tauri.js';

function gesture(g: ConsoleGesture): void {
  call('console', { gesture: g });
}

/** A button's title with its key from the keymap, as the nav's are. */
function titled(words: string, action: 'console' | 'toggleConsole' | 'consoleFullscreen'): string {
  const chords = chordsFor(action, action === 'consoleFullscreen' ? 'pane' : 'anywhere');
  const keys = chords.map((chord) => presses(chord).join(' ')).join(', ');
  return keys ? `${words} (${keys})` : words;
}

const PLACES = ['hidden', 'docked', 'tab', 'fullscreen'] as const;

export function drawConsole(): void {
  const page = state.console;
  const place = page?.place ?? 'hidden';
  for (const p of PLACES) document.body.classList.toggle(`console-${p}`, place === p);
  $('console').hidden = place === 'hidden';
  $('console-sash').hidden = place !== 'docked';
  if (page) $('console').style.height = place === 'docked' ? `${page.height}px` : '';
  drawHead();
  placeTerminal();
}

function drawHead(): void {
  const page = state.console;
  if (!page) {
    $('console-head').replaceChildren();
    return;
  }
  const docked = page.place === 'docked';
  const full = page.place === 'fullscreen';
  $('console-head').replaceChildren(
    el('span', { class: 'label' }, 'Console'),
    el(
      'div',
      { class: 'ctabs', role: 'tablist' },
      page.tabs.map((tab) =>
        el(
          'div',
          { class: `ctab${tab.on ? ' on' : ''}${tab.pane === page.showing ? ' live' : ''}` },
          el(
            'button',
            {
              class: 'ctab-pick',
              role: 'tab',
              'aria-selected': String(tab.on),
              title: `shell, ${tab.status}`,
              onclick: () => gesture({ on: 'select', pane: tab.pane }),
            },
            el('span', { class: 'glyph sh' }, '›'),
            el('span', { class: 'ttl' }, tab.title),
            el('span', { class: `sdot ${tab.status}`, 'aria-hidden': 'true' }),
          ),
          el(
            'button',
            {
              class: 'icon x',
              title: 'Close',
              'aria-label': `Close ${tab.title}`,
              onclick: () => gesture({ on: 'close', pane: tab.pane }),
            },
            icon('close'),
          ),
        ),
      ),
    ),
    page.canSpawn
      ? el(
          'button',
          {
            class: 'icon',
            title: 'New console',
            'aria-label': 'New console',
            onclick: () => gesture({ on: 'spawn' }),
          },
          icon('plus'),
        )
      : '',
    el('span', { class: 'grow' }),
    docked
      ? el(
          'button',
          {
            class: 'icon',
            title: titled('Open in the Console stage', 'console'),
            'aria-label': 'Open in the Console stage',
            onclick: () => gesture({ on: 'tab' }),
          },
          icon('up'),
        )
      : el(
          'button',
          {
            class: 'icon',
            title: 'Dock under the stage',
            'aria-label': 'Dock under the stage',
            onclick: () => gesture({ on: 'dock' }),
          },
          icon('down'),
        ),
    el(
      'button',
      {
        class: `icon${full ? ' hot' : ''}`,
        title: titled(full ? 'Leave fullscreen' : 'Fullscreen', 'consoleFullscreen'),
        'aria-label': 'Fullscreen',
        'aria-pressed': String(full),
        onclick: () => gesture({ on: 'fullscreen' }),
      },
      icon('expand'),
    ),
    el(
      'button',
      {
        class: 'icon',
        title: titled('Hide the console', 'toggleConsole'),
        'aria-label': 'Hide the console',
        onclick: () => gesture({ on: 'hide' }),
      },
      icon('close'),
    ),
  );
}

/** The canvas and its keys go where the pane they draw is shown: in the
 *  console while it draws a console, and in the Workspace otherwise. */
export function placeTerminal(): void {
  const terminal = $('terminal');
  const page = state.console;
  const inConsole = page?.showing != null && page.place !== 'hidden';
  const home = inConsole ? $('console-body') : $('body');
  if (terminal.parentElement !== home) {
    const hadKeys = document.activeElement === $('input');
    home.prepend(terminal);
    if (hadKeys) $('input').focus();
  }
  const empty = $('console-empty');
  empty.hidden = inConsole;
  if (inConsole || !page) return;
  empty.replaceChildren(
    page.tabs.length === 0
      ? el(
          'div',
          { class: 'empty-body' },
          el(
            'div',
            { class: 'fg-muted' },
            'No consoles yet: a shell in your home directory, in no checkout.',
          ),
          page.canSpawn &&
            el(
              'button',
              { class: 'btn', onclick: () => gesture({ on: 'spawn' }) },
              'Start a console',
            ),
        )
      : el(
          'button',
          {
            class: 'btn',
            onclick: () => {
              const on = page.tabs.find((tab) => tab.on) ?? page.tabs[0];
              if (on) gesture({ on: 'select', pane: on.pane });
            },
          },
          'Type in the console',
        ),
  );
}

/** The sash: the console follows the pointer while it is held, and its
 *  height is reported once it is let go, for Rust to keep and send back. */
export function listen(): void {
  const sash = $('console-sash');
  sash.addEventListener('mousedown', (e) => {
    if (e.button !== 0) return;
    e.preventDefault();
    const stage = $('console').parentElement;
    if (!stage) return;
    const bottom = stage.getBoundingClientRect().bottom;
    let height = $('console').offsetHeight;
    const move = (m: MouseEvent): void => {
      height = Math.round(bottom - m.clientY);
      $('console').style.height = `${height}px`;
    };
    const up = (): void => {
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
