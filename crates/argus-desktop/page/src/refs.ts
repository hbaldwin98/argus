// The values that point into Argus rather than out of it — a path, a
// commit, a feature, a pane — and the plain ones drawn beside them: a
// duration and inline code.

import type { Value } from './bindings/Value';
import { el } from './dom.js';

export type RefValue = Extract<
  Value,
  | { kind: 'path' }
  | { kind: 'commit' }
  | { kind: 'feature' }
  | { kind: 'pane' }
  | { kind: 'duration' }
  | { kind: 'code' }
>;

/** Not drawn yet: the value's kind. */
export function refValue(v: RefValue, pane: number | null): HTMLElement {
  void pane;
  return el('span', { class: 'vpending' }, v.kind);
}
