// Naming a pane or a console: a dialog in the layer, asked of the daemon
// on Enter. The row keeps its old name until the daemon's tree says the
// new one, so a refused rename leaves it showing what is true.

import { $, el } from './dom.js';
import { actionFor } from './keymap.js';
import { closeLayer } from './menu.js';
import { call } from './tauri.js';

export function renameDialog(pane: number, title: string): void {
  const name = el('input', { class: 'field', id: 'rename-name', value: title });
  const rename = (): void => {
    const next = name.value.trim();
    closeLayer();
    if (next && next !== title) call('rename_pane', { pane, title: next });
  };
  name.addEventListener('keydown', (e) => {
    const action = actionFor(['writing'], e);
    if (action === 'choose') rename();
    if (action === 'closeLayer') closeLayer();
  });
  $('layer').replaceChildren(
    el(
      'div',
      {
        class: 'scrim',
        onclick: (e: MouseEvent) => {
          if (e.target === e.currentTarget) closeLayer();
        },
      },
      el(
        'section',
        {
          class: 'dialog hold-dialog',
          role: 'dialog',
          'aria-modal': 'true',
          'aria-labelledby': 'rename-title',
        },
        el(
          'div',
          { class: 'dhead' },
          el('div', { class: 'type-title', id: 'rename-title' }, `Rename ${title}`),
        ),
        el(
          'div',
          { class: 'dbody' },
          el('label', { class: 'label', for: 'rename-name' }, 'Name'),
          name,
          el('div', { class: 'meta' }, 'An agent may rename its own pane later.'),
        ),
        el(
          'div',
          { class: 'dfoot' },
          el('span', { class: 'grow' }),
          el('button', { class: 'btn', onclick: closeLayer }, 'Cancel'),
          el('button', { class: 'btn primary', onclick: rename }, 'Rename'),
        ),
      ),
    ),
  );
  name.focus();
  name.select();
}
