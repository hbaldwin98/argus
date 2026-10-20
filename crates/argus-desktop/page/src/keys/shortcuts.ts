// The shortcuts sheet: every binding of the keymap Rust sends, under the
// heading of where it works, so it lists exactly the keys dispatch runs.

import { $, el, focusQuietly, icon, sentence } from '../kit/dom.js';
import { extensionSheetGroups } from '../extensions/views.js';
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

/** Deals the groups into three columns, each under the shortest so far, so a
 *  heading always tops its rows: CSS columns split a group taller than the
 *  sheet across two. */
function columns(groups: HTMLElement[]): HTMLElement[] {
  const cols = [0, 1, 2].map(() => ({ rows: 0, node: el('div', { class: 'sheet-col' }) }));
  for (const group of groups) {
    const col = cols.reduce((a, b) => (b.rows < a.rows ? b : a));
    col.node.append(group);
    col.rows += group.querySelectorAll('.sheet-row').length + 2;
  }
  return cols.map((c) => c.node);
}

/** The first chord alone when a binding has many: a row of three alternatives
 *  squeezes its label; the whole list is in the tooltip. */
function keys(chords: Parameters<typeof keycaps>[0]): HTMLElement {
  const caps = keycaps(chords.length > 2 ? chords.slice(0, 1) : chords);
  if (chords.length > 2) caps.title = keycaps(chords).textContent ?? '';
  return caps;
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
          el('span', { class: 'grow' }, sentence(binding.label)),
          keys(binding.chords),
        ),
      ),
    ),
  );
  const close = el(
    'button',
    { class: 'icon', 'aria-label': 'Close shortcuts', onclick: closeSheet },
    icon('close'),
  );
  $('over').replaceChildren(
    el(
      'div',
      {
        class: 'scrim',
        onmousedown: (e: MouseEvent) => {
          if (e.target === e.currentTarget) closeSheet();
        },
      },
      el(
        'section',
        {
          class: 'dialog sheet-dialog sheet',
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
        el('div', { class: 'sheet-body' }, columns([...groups, ...extensionSheetGroups()])),
        el(
          'div',
          { class: 'dfoot' },
          el(
            'span',
            { class: 'dwhy grow' },
            'Keys inside a pane go to the pane, except the Everywhere group and ',
            el('span', { class: 'kbd' }, lead),
            ' then a key',
          ),
        ),
      ),
    ),
  );
  const body = $('over').querySelector<HTMLElement>('.sheet-body');
  if (body) {
    const fade = (): void => {
      body.classList.toggle('more', body.scrollTop + body.clientHeight < body.scrollHeight - 1);
    };
    body.addEventListener('scroll', fade, { passive: true });
    new ResizeObserver(fade).observe(body);
    fade();
  }
  // The sheet holds the keys while open; Escape or ? puts it away.
  $('input').blur();
  focusQuietly(close);
}

export function closeSheet(): void {
  if (!sheetOpen()) return;
  $('over').replaceChildren();
  if (before instanceof HTMLElement) before.focus();
  before = null;
}
