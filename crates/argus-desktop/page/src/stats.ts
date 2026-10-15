// A stats view block: tiles of an already-formatted value under a label, with
// an optional delta coloured by the stat's tone.

import type { Stat } from './bindings/Stat';
import { toneClass } from './chip.js';
import { el } from './dom.js';

export function stats(list: readonly Stat[]): HTMLElement {
  return el(
    'div',
    { class: 'vstats' },
    list.map((s) =>
      el(
        'div',
        { class: `vstat ${toneClass(s.tone)}` },
        el('span', { class: 'vstat-label' }, s.label),
        el('span', { class: 'vstat-value' }, s.value),
        s.delta != null
          ? el(
              'span',
              { class: s.tone === 'neutral' ? 'vstat-delta muted' : 'vstat-delta' },
              s.delta,
            )
          : null,
      ),
    ),
  );
}
