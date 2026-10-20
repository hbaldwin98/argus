// A font field: a family typed, or picked from the installed ones listed
// under it, each drawn in its own face. Typing narrows the list. A name is
// chosen when it is picked, on Enter or when the field is left, so a
// half-typed one never redraws the window in the fallback.

import { el, icon } from '../kit/dom.js';
import { actionFor } from '../keys/keymap.js';

export type FontPicker = {
  node: HTMLElement;
  /** The families to offer and the one chosen. A field being typed in is
   *  ahead of what came back, so it keeps its text. */
  update(families: string[], chosen: string): void;
  /** Renames what an empty field draws, which a theme changes. */
  defaultName(name: string): void;
};

type Row = { family: string; label: string } | { note: string };

/** `fallback` is the CSS stack drawn when the field is empty, and is named
 *  by `fallbackName` in the list's first row; the name can change later. */
export function fontPicker(
  label: string,
  fallback: string,
  fallbackName: string,
  choose: (family: string) => void,
): FontPicker {
  let families: string[] = [];
  let chosen = '';
  // Whether the list is narrowed by what is typed, rather than shown whole
  // around the name the field opened on.
  let typed = false;
  let active = 0;
  let rows: Row[] = [];

  const input = el('input', {
    class: 'field',
    role: 'combobox',
    'aria-label': label,
    'aria-expanded': 'false',
    autocomplete: 'off',
    spellcheck: 'false',
    placeholder: fallbackName,
  });
  const list = el('div', { class: 'menu fontpick-list', role: 'listbox', hidden: true });
  const node = el(
    'div',
    { class: 'fontpick' },
    input,
    el('span', { class: 'fontpick-chev' }, icon('down')),
    list,
  );

  const face = (family: string): string => (family ? `"${family}", ${fallback}` : fallback);
  const isOpen = (): boolean => !list.hidden;

  function draw(): void {
    const query = typed ? input.value.trim().toLowerCase() : '';
    const matching = families.filter((f) => f.toLowerCase().includes(query));
    rows = [];
    if (!query) rows.push({ family: '', label: fallbackName });
    for (const family of matching) rows.push({ family, label: family });
    if (families.length === 0) rows.push({ note: 'Reading the installed fonts…' });
    else if (matching.length === 0)
      rows.push({ note: 'None installed matches; Enter uses it as typed' });
    active = Math.max(0, Math.min(active, rows.length - 1));
    list.replaceChildren(
      ...rows.map((row, i) => {
        if ('note' in row)
          return el('div', { class: 'mi fontpick-note', 'aria-disabled': 'true' }, row.note);
        const button = el(
          'button',
          {
            class: `mi${i === active ? ' on' : ''}`,
            type: 'button',
            role: 'option',
            tabindex: '-1',
            'aria-selected': String(row.family === chosen),
            // Keeps the keys in the field, which a click would take.
            onmousedown: (e: MouseEvent) => e.preventDefault(),
            onclick: () => pick(row.family),
          },
          el('span', { class: 'grow' }, row.label),
          row.family === chosen ? el('span', { class: 'fontpick-check' }, '✓') : null,
        );
        button.style.fontFamily = face(row.family);
        return button;
      }),
    );
    list.querySelector('.mi.on')?.scrollIntoView({ block: 'nearest' });
  }

  function open(): void {
    if (isOpen()) return;
    typed = false;
    list.hidden = false;
    input.setAttribute('aria-expanded', 'true');
    draw();
    // Opened on the chosen family, so the list starts where the person is.
    const at = rows.findIndex((row) => 'family' in row && row.family === chosen);
    move(at < 0 ? 0 : at);
  }

  function close(): void {
    list.hidden = true;
    input.setAttribute('aria-expanded', 'false');
  }

  function move(to: number): void {
    const choosable = rows.flatMap((row, i) => ('family' in row ? [i] : []));
    if (choosable.length === 0) return;
    active = Math.max(choosable[0]!, Math.min(choosable[choosable.length - 1]!, to));
    draw();
  }

  function pick(family: string): void {
    input.value = family;
    close();
    if (family !== chosen) choose(family);
  }

  function commitTyped(): void {
    const family = input.value.trim();
    if (typed && family !== chosen) choose(family);
    typed = false;
  }

  input.addEventListener('focus', open);
  input.addEventListener('click', open);
  input.addEventListener('input', () => {
    if (!isOpen()) open();
    typed = true;
    active = 0;
    draw();
  });
  input.addEventListener('blur', () => {
    commitTyped();
    close();
    input.value = chosen;
  });
  input.addEventListener('keydown', (e) => {
    const action = actionFor(['palette'], e);
    if (action === 'next' || action === 'previous') {
      e.preventDefault();
      if (!isOpen()) open();
      else move(active + (action === 'next' ? 1 : -1));
    } else if (action === 'choose') {
      e.preventDefault();
      const row = rows[active];
      if (isOpen() && row && 'family' in row) {
        pick(row.family);
      } else {
        commitTyped();
        close();
      }
    } else if (action === 'closeLayer' && isOpen()) {
      // Closes the list rather than the dialog, which the next Escape does.
      e.preventDefault();
      e.stopPropagation();
      typed = false;
      input.value = chosen;
      close();
    }
  });

  return {
    node,
    defaultName(name) {
      if (name === fallbackName) return;
      fallbackName = name;
      input.placeholder = name;
      if (isOpen()) draw();
    },
    update(next, nowChosen) {
      chosen = nowChosen;
      input.style.fontFamily = face(chosen);
      if (document.activeElement !== input) input.value = chosen;
      const changed = next.join('\n') !== families.join('\n');
      families = next;
      if (isOpen() && changed) draw();
    },
  };
}
