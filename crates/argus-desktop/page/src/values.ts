// STUB: the values agent replaces this file.

import type { Value } from './bindings/Value';
import { el } from './dom.js';

/** One value, as a field, a cell or a list item shows it. */
export function value(v: Value): HTMLElement {
  return el('span', null, v.kind);
}

/** A person: initials in a circle, and the name. */
export function person(name: string): HTMLElement {
  return el('span', { class: 'vperson' }, name);
}

/** How long ago an RFC 3339 time was, `4 min ago`; the text as given when it is not one. */
export function ago(at: string): string {
  return at;
}
