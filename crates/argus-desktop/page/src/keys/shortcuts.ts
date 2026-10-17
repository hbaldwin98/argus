// The shortcuts sheet: every binding of the keymap Rust sends, under the
// heading of where it works, so it lists exactly the keys dispatch runs.

import { $, el } from '../kit/dom.js';
import { extensionSheetGroups } from '../extension_views.js';
import { keycaps, leader } from './chords.js';
import { state } from '../state.js';

/** What had the keys before the sheet, to give them back to. */
let before: Element | null = null;

export function sheetOpen(): boolean {
  return $('over').querySelector('.sheet') !== null;
}

export function toggleSheet(): void {
  if (sheetOpen()) closeSheet();
  else openSheet();
}

export function openSheet(): void {
  before = document.activeElement;
  const lead = leader();
  const groups = state.keymap.map((group) =>
    el(
      'section',
      { class: 'sheet-group' },
      el('div', { class: 'mlabel' }, group.title),
      group.bindings.map((binding) =>
        el(
          'div',
          { class: 'sheet-row' },
          el('span', { class: 'grow' }, binding.label),
          keycaps(binding.chords),
        ),
      ),
    ),
  );
  const close = el('button', { class: 'btn sm', onclick: closeSheet }, 'Close');
  $('over').replaceChildren(
    el(
      'div',
      {
        class: 'scrim top',
        onmousedown: (e: MouseEvent) => {
          if (e.target === e.currentTarget) closeSheet();
        },
      },
      el(
        'section',
        {
          class: 'dialog sheet',
          role: 'dialog',
          'aria-modal': 'true',
          'aria-labelledby': 'sheet-title',
        },
        el(
          'div',
          { class: 'dhead' },
          el('div', { class: 'type-title grow', id: 'sheet-title' }, 'Keyboard shortcuts'),
          close,
        ),
        el(
          'div',
          { class: 'sheet-rule' },
          'Inside a pane every key goes to the pane, except those listed under Everywhere and ',
          el('span', { class: 'kbd' }, lead),
          ': the key after it is the window’s, and ',
          el('span', { class: 'kbd' }, lead),
          ' twice sends one through.',
        ),
        el('div', { class: 'sheet-body' }, groups, extensionSheetGroups()),
      ),
    ),
  );
  // The sheet holds the keys while open; Escape or ? puts it away.
  $('input').blur();
  close.focus();
}

export function closeSheet(): void {
  if (!sheetOpen()) return;
  $('over').replaceChildren();
  if (before instanceof HTMLElement) before.focus();
  before = null;
}
