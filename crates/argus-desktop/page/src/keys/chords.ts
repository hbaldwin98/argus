// How a key is written: the shortcuts sheet, the palette and the status
// bar's hint say each chord of the keymap Rust sends the same way.

import type { Action } from '../bindings/Action';
import type { Chord } from '../bindings/Chord';
import type { Place } from '../bindings/Place';
import { el } from '../kit/dom.js';
import { state } from '../state.js';

const MAC = /Macintosh|Mac OS X/.test(navigator.userAgent);

const NAMES: Record<string, string> = {
  ' ': 'Space',
  ArrowDown: '↓',
  ArrowUp: '↑',
  ArrowLeft: '←',
  ArrowRight: '→',
  Escape: 'Esc',
  PageUp: 'PgUp',
  PageDown: 'PgDn',
};

/** One press, its modifiers first: `Ctrl K`, `Shift PgUp`, `j`. */
function press(chord: Chord): string {
  const name = NAMES[chord.key] ?? chord.key;
  const modifiers = [
    chord.ctrl && 'Ctrl',
    chord.alt && (MAC ? '⌥' : 'Alt'),
    chord.shift && 'Shift',
    chord.meta && (MAC ? '⌘' : 'Meta'),
  ].filter((m): m is string => !!m);
  // A letter held with a modifier is written as on the key; alone it is
  // the character typed, so shift-a is `A`.
  const key = modifiers.length && [...name].length === 1 ? name.toUpperCase() : name;
  return [...modifiers, key].join(' ');
}

/** The presses a chord takes: the leader's first when it follows one. */
export function presses(chord: Chord): string[] {
  return chord.leader ? [leader(), press(chord)] : [press(chord)];
}

/** The leader, as the pane's binding that sends it through holds it. */
export function leader(): string {
  const pane = state.keymap.find((group) => group.place === 'pane');
  const chord = pane?.bindings.find((binding) => binding.action === 'leader')?.chords[0];
  return chord ? press({ ...chord, leader: false }) : 'Ctrl Space';
}

/** A binding's chords as keycaps, the alternatives side by side. */
export function keycaps(chords: readonly Chord[]): HTMLElement {
  return el(
    'span',
    { class: 'keys' },
    chords.map((chord, i) => [
      i > 0 && el('span', { class: 'fg-faint' }, '/'),
      presses(chord).map((p) => el('span', { class: 'kbd' }, p)),
    ]),
  );
}

/** The chords bound to `action` in `place`. */
export function chordsFor(action: Action, place: Place): Chord[] {
  const group = state.keymap.find((g) => g.place === place);
  return group?.bindings.find((binding) => binding.action === action)?.chords ?? [];
}
