// The history dialog: a checkout's newest commits, each unfolding in
// place to the files it touched, beside the diff of the file under the
// cursor. Rust holds the list and the diffs (`src/review/history.rs`); the
// page draws them and says what was pressed.

import type { HistoryPage } from '../bindings/HistoryPage';
import type { HistoryRowView } from '../bindings/HistoryRowView';
import { $, el, ghosts, icon, type Child } from '../kit/dom.js';
import { actionFor } from '../keys/keymap.js';
import { diffRow } from './dialog.js';
import { state } from '../state.js';
import { call } from '../connection/tauri.js';

function keycap(key: string): HTMLElement {
  return el('span', { class: 'kbd' }, key);
}

function listRow(r: HistoryRowView): HTMLElement {
  const pick = (): void => call('history_action', { action: 'next', row: r.row });
  if (r.kind === 'file') {
    return el(
      'button',
      {
        class: `hrow file${r.on ? ' on' : ''}`,
        onclick: pick,
        ondblclick: () => call('history_action', { action: 'choose', row: null }),
      },
      el('span', { class: 'mono grow ellipsis' }, r.text),
      el('span', { class: 'fg-ok' }, `+${r.added}`),
      el('span', { class: 'fg-danger' }, `−${r.removed}`),
    );
  }
  return el(
    'button',
    {
      class: `hrow${r.on ? ' on' : ''}${r.expanded ? ' open' : ''}`,
      onclick: pick,
      ondblclick: () => call('history_action', { action: 'unfold', row: null }),
    },
    el('span', { class: 'mono fg-faint' }, r.short),
    el('span', { class: 'grow ellipsis' }, r.text),
    r.loading
      ? el('span', { class: 'meta' }, 'loading…')
      : r.expanded &&
          r.files !== null &&
          el('span', { class: 'meta' }, `${r.files} file${r.files === 1 ? '' : 's'}`),
    el('span', { class: 'meta' }, r.ago),
  );
}

function diff(h: HistoryPage): HTMLElement {
  let body: Child;
  if (!h.path)
    body = el('div', { class: 'review-empty fg-muted' }, 'Open a commit and pick a file.');
  else if (h.diffEmpty) body = el('div', { class: 'review-empty fg-muted' }, h.diffEmpty);
  else if (h.diffLoading) body = ghosts([[0.4], [0.9], [0.8], [0.85], [0.6]]);
  else body = h.diff.map((d) => diffRow(d));
  return el(
    'section',
    { class: 'review-diff' },
    h.path &&
      el(
        'div',
        { class: 'fhdr' },
        el('span', { class: 'mono fg-faint' }, h.commit ?? ''),
        el('span', { class: 'mono' }, h.path),
        el('span', { class: 'meta' }, 'against its first parent'),
        el('span', { class: 'grow' }),
        el(
          'button',
          {
            class: 'btn sm',
            onclick: () => call('history_action', { action: 'asReview', row: null }),
          },
          'Open as a review ',
          keycap('O'),
        ),
      ),
    el('div', { class: 'dbody', id: 'history-body' }, body),
  );
}

/**
 * Rust filters the rows; the field only says what it says. Its text is its
 * own once drawn: an echo that lands after the next letter would drop it.
 */
function filterField(filter: string): HTMLElement {
  const live = document.getElementById('history-filter');
  const field = el('input', {
    class: 'field filter',
    id: 'history-filter',
    placeholder: 'Filter commits',
    'aria-label': 'Filter commits by subject, hash or author',
    oninput: (e: Event) => {
      if (e.target instanceof HTMLInputElement) call('filter_history', { filter: e.target.value });
    },
    onkeydown: (e: KeyboardEvent) => {
      // The list has the keys back, filtered.
      if (actionFor(['filter'], e) === 'leave') {
        e.preventDefault();
        (e.target as HTMLElement).blur();
      }
    },
  });
  field.value = live instanceof HTMLInputElement ? live.value : filter;
  return field;
}

/** `/`: the filter field takes the keys. */
export function focusHistoryFilter(): void {
  document.getElementById('history-filter')?.focus();
}

export function drawHistory(): void {
  const h = state.history;
  const host = $('history');
  if (!h?.open) {
    host.hidden = true;
    host.replaceChildren();
    return;
  }
  const filterHad = document.activeElement?.id === 'history-filter';
  const scrolled = ['history-list', 'history-body'].map(
    (id) => document.getElementById(id)?.scrollTop ?? 0,
  );
  host.hidden = false;
  let list: Child;
  if (h.loading) list = ghosts([[0.8], [0.7], [0.9], [0.6], [0.75], [0.85]]);
  else if (h.empty) list = el('div', { class: 'review-empty fg-muted' }, h.empty);
  else list = h.rows.map(listRow);
  host.replaceChildren(
    el(
      'div',
      { class: 'scrim' },
      el(
        'section',
        {
          class: 'dialog sheet-dialog review-dialog',
          role: 'dialog',
          'aria-modal': 'true',
          'aria-labelledby': 'history-title',
        },
        el(
          'div',
          { class: 'dhead review-head' },
          el(
            'div',
            { class: 'type-title', id: 'history-title' },
            'History · ',
            h.name,
            ' ',
            el('span', { class: 'mono fg-ref' }, h.branch),
          ),
          el('span', { class: 'meta' }, 'newest 100 on HEAD'),
          el('span', { class: 'grow' }),
          filterField(h.filter),
          el(
            'button',
            {
              class: 'icon',
              'aria-label': 'Close the history',
              onclick: () => call('history_action', { action: 'closeLayer', row: null }),
            },
            icon('close'),
          ),
        ),
        el(
          'div',
          { class: 'review-split' },
          el('aside', { class: 'history-list', id: 'history-list', 'aria-label': 'Commits' }, list),
          diff(h),
        ),
        el(
          'div',
          { class: 'dfoot meta' },
          keycap('J'),
          keycap('K'),
          ' move · ',
          keycap('L'),
          ' open · ',
          keycap('H'),
          ' fold · ',
          keycap('Enter'),
          ' file as a review · ',
          keycap('/'),
          ' filter · ',
          keycap('Esc'),
          ' close',
        ),
      ),
    ),
  );
  ['history-list', 'history-body'].forEach((id, i) => {
    const node = document.getElementById(id);
    if (node) node.scrollTop = scrolled[i] ?? 0;
  });
  host.querySelector('.hrow.on')?.scrollIntoView({ block: 'nearest' });
  // Redrawn as each letter arrives: the filter keeps the keys and the caret.
  const field = document.getElementById('history-filter');
  if (filterHad && field instanceof HTMLInputElement) {
    field.focus();
    field.setSelectionRange(field.value.length, field.value.length);
  }
}

export function historyOpen(): boolean {
  return state.history?.open === true;
}
