// The shortcuts sheet: every binding of the keymap Rust sends, a section per
// place with its keys beside it, opened on the section for where the person
// is, so it lists exactly the keys dispatch runs.

import type { Action } from '../bindings/Action';
import { $, el, focusQuietly, icon, sentence } from '../kit/dom.js';
import { leader, press } from './chords.js';
import { matching, sectionFor, sections, type Row, type Section, type Where } from './sections.js';
import { state } from '../state.js';

/** What had the keys before the sheet, to give them back to. */
let before: Element | null = null;

export function sheetOpen(): boolean {
  return $('over').querySelector('.sheet') !== null;
}

export function toggleSheet(where: Where, actionOf: ActionOf): void {
  if (sheetOpen()) closeSheet();
  else openSheet(where, actionOf);
}

/** Every section the sheet has: the keymap's, then one per extension. */
function allSections(): Section[] {
  const extensions = (state.extensions?.groups ?? []).map((group) => ({
    place: `ext/${group.title}`,
    title: group.title,
    bindings: group.bindings,
  }));
  return sections([...state.keymap, ...extensions], {
    press,
    leader: leader(),
  });
}

/** The first key alone when a binding has many: a row of three alternatives
 *  squeezes its label; the whole list is in the tooltip. */
function keys(row: Row): HTMLElement {
  const shown = row.keys.length > 2 ? row.keys.slice(0, 1) : row.keys;
  const caps = el(
    'span',
    { class: 'keys' },
    shown.map((alternative, i) => [
      i > 0 && el('span', { class: 'fg-faint' }, '/'),
      alternative.map((p) => el('span', { class: 'kbd' }, p)),
    ]),
  );
  if (row.keys.length > 2) caps.title = row.keys.map((k) => k.join(' ')).join(' / ');
  return caps;
}

function rowNode(row: Row): HTMLElement {
  return el(
    'div',
    { class: 'sheet-row' },
    el('span', { class: 'grow' }, sentence(row.label)),
    keys(row),
  );
}

/** What the pane section says once, in place of on every row. */
function paneHead(): HTMLElement {
  return el(
    'div',
    { class: 'ks-note' },
    'Keys go to the program. For Argus, press ',
    el('span', { class: 'kbd' }, leader()),
    ' then one key:',
  );
}

function sectionBody(section: Section, titled: boolean): HTMLElement {
  return el(
    'section',
    { class: 'ks-section' },
    titled && el('h3', { class: 'ks-title' }, section.title),
    section.chorded && section.rows.length > 0 && paneHead(),
    el('div', { class: section.chorded ? 'ks-rows two' : 'ks-rows' }, section.rows.map(rowNode)),
    section.without.length > 0 &&
      el(
        'div',
        { class: 'ks-without' },
        el('div', { class: 'mlabel' }, `Without ${leader()}`),
        el('div', { class: 'ks-rows two' }, section.without.map(rowNode)),
      ),
  );
}

/** What the keymap makes of a key in the sheet, which the page's own
 *  dispatch cannot say: a field swallows keys before the window hears them. */
export type ActionOf = (e: KeyboardEvent) => Action | null;

export function openSheet(where: Where, actionOf: ActionOf): void {
  before = document.activeElement;
  const all = allSections();
  let current = sectionFor(
    where,
    all.map((s) => s.id),
  );
  const here = current;
  let query = '';

  const nav = el('nav', { class: 'ks-nav', 'aria-label': 'Sections' });
  const pane = el('div', { class: 'ks-pane sheet-body' });
  const find = el('input', {
    class: 'ks-find',
    type: 'text',
    placeholder: 'Find a key or an action',
    'aria-label': 'Find a key or an action',
    spellcheck: 'false',
    autocomplete: 'off',
    oninput: (e: Event) => {
      query = (e.target as HTMLInputElement).value;
      draw();
    },
  });

  const draw = (): void => {
    const found = matching(all, query);
    const filtering = query.trim() !== '';
    nav.replaceChildren(
      ...found.map((s) =>
        el(
          'button',
          {
            class: !filtering && s.id === current ? 'ks-item on' : 'ks-item',
            'aria-current': !filtering && s.id === current ? 'true' : null,
            onclick: () => {
              find.value = query = '';
              current = s.id;
              draw();
              // The list is rebuilt, so the clicked button is gone; without
              // this the keys fall to the page and Up/Down stop working.
              focusQuietly(nav.querySelector<HTMLElement>('.on'));
            },
          },
          el('span', { class: 'grow' }, s.title),
          s.id === here && el('span', { class: 'ks-here' }, 'here'),
        ),
      ),
    );
    pane.replaceChildren(
      ...(filtering
        ? found.map((s) => sectionBody(s, true))
        : [sectionBody(all.find((s) => s.id === current) ?? all[0] ?? emptySection, true)]),
    );
    if (found.length === 0)
      pane.replaceChildren(el('div', { class: 'meta ks-none' }, 'No key or action matches.'));
    pane.scrollTop = 0;
    nav.querySelector('.on')?.scrollIntoView({ block: 'nearest' });
  };

  // Next and previous change section, from the field or the list alike;
  // the field swallows keys before the window hears them, so close is
  // taken here too.
  const keydown = (e: KeyboardEvent): void => {
    const action = actionOf(e);
    if (action === 'closeLayer') {
      e.preventDefault();
      e.stopPropagation();
      closeSheet();
    } else if ((action === 'next' || action === 'previous') && query.trim() === '') {
      const at = all.findIndex((s) => s.id === current);
      const next = all[Math.min(all.length - 1, Math.max(0, at + (action === 'next' ? 1 : -1)))];
      e.preventDefault();
      e.stopPropagation();
      if (next) {
        current = next.id;
        draw();
      }
    }
  };

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
          class: 'dialog sheet-dialog sheet keys-sheet',
          role: 'dialog',
          'aria-modal': 'true',
          'aria-labelledby': 'sheet-title',
          onkeydown: keydown,
        },
        el(
          'div',
          { class: 'dhead' },
          el('div', { class: 'type-title grow', id: 'sheet-title' }, 'Keyboard shortcuts'),
          close,
        ),
        el(
          'div',
          { class: 'ks-main' },
          el(
            'div',
            { class: 'ks-side' },
            el('label', { class: 'ks-search' }, icon('search'), find),
            nav,
          ),
          pane,
        ),
        el(
          'div',
          { class: 'dfoot ks-foot' },
          el('span', { class: 'kbd' }, '?'),
          'opens on the section for where you are · ',
          el('span', { class: 'kbd' }, '↑'),
          el('span', { class: 'kbd' }, '↓'),
          'change section · ',
          el('span', { class: 'kbd' }, 'Esc'),
          'close',
        ),
      ),
    ),
  );
  draw();
  // The sheet holds the keys while open; Escape or ? puts it away.
  $('input').blur();
  focusQuietly(close);
}

const emptySection: Section = { id: '', title: '', rows: [], without: [], chorded: false };

export function closeSheet(): void {
  if (!sheetOpen()) return;
  $('over').replaceChildren();
  if (before instanceof HTMLElement) before.focus();
  before = null;
}
