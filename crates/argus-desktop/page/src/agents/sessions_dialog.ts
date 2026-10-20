// The recent-sessions dialog: a repository's agent sessions, live ones
// first, beside what the one under the cursor said, with the way to go to
// it, resume it, or read it whole in the reader `transcript` draws in
// place of the list. Rust holds the list, the filters and what each
// session offers (`src/agents/sessions_dialog.rs`); the page draws them
// and says what was pressed.

import type { Action } from '../bindings/Action';
import type { SessionPreview } from '../bindings/SessionPreview';
import type { SessionRowView } from '../bindings/SessionRowView';
import type { SessionsPage } from '../bindings/SessionsPage';
import { ago } from '../kit/clock.js';
import { $, el, ghosts, icon, type Child } from '../kit/dom.js';
import { actionFor } from '../keys/keymap.js';
import { state } from '../state.js';
import { call } from '../connection/tauri.js';
import { drawReader, placeReader, said } from './transcript.js';

function keycap(key: string): HTMLElement {
  return el('span', { class: 'kbd' }, key);
}

function act(action: Action): void {
  call('sessions_action', { action, row: null });
}

function listRow(r: SessionRowView): HTMLElement {
  const turns = r.turns === null ? null : `${r.turns} turn${r.turns === 1 ? '' : 's'}`;
  return el(
    'button',
    {
      class: `hrow srow${r.on ? ' on' : ''}${r.removed ? ' removed' : ''}`,
      title: r.removed ? `ran in ${r.checkout}, a worktree since removed` : null,
      onclick: () => call('sessions_action', { action: 'next', row: r.row }),
      ondblclick: () => act('choose'),
    },
    el('span', { class: `sdot${r.live ? ' live' : ''}`, 'aria-hidden': 'true' }),
    el('span', { class: 'grow ellipsis' }, r.title),
    el('span', { class: 'meta' }, r.harness),
    el('span', { class: 'meta mono ellipsis scheckout' }, r.checkout),
    r.removed && el('span', { class: 'meta' }, 'removed'),
    turns && el('span', { class: 'meta' }, turns),
    el('span', { class: 'meta' }, r.live ? 'live' : r.ago),
  );
}

/** The rows under a heading for the live ones and one for the rest. */
function list(s: SessionsPage): Child {
  if (s.loading) return ghosts([[0.8], [0.7], [0.9], [0.6], [0.75], [0.85]]);
  if (s.empty) return el('div', { class: 'review-empty fg-muted' }, s.empty);
  const live = s.rows.filter((r) => r.live);
  const ended = s.rows.filter((r) => !r.live);
  return [
    live.length > 0 && el('div', { class: 'mlabel' }, 'Live'),
    live.map(listRow),
    ended.length > 0 && el('div', { class: 'mlabel' }, 'Ended'),
    ended.map(listRow),
  ];
}

function transcript(p: SessionPreview): Child {
  if (p.loading) return ghosts([[0.5], [0.9], [0.8], [0.4], [0.85], [0.6]]);
  if (p.notice) return el('div', { class: 'review-empty fg-muted' }, p.notice);
  return [
    p.earlier > 0 &&
      el(
        'div',
        { class: 'meta tearlier' },
        `${p.earlier} earlier entr${p.earlier === 1 ? 'y' : 'ies'} not shown`,
      ),
    p.entries.map((l) => said(l)),
  ];
}

function preview(s: SessionsPage): HTMLElement {
  const p = s.preview;
  return el(
    'section',
    { class: 'review-diff' },
    p &&
      el(
        'div',
        { class: 'fhdr' },
        el('span', { class: 'ellipsis' }, p.title),
        el('span', { class: 'meta' }, p.harness),
        el('span', { class: 'meta mono' }, p.checkout),
        p.started &&
          el('span', { class: 'meta nowrap', title: p.started }, ago(p.started) ?? p.started),
        el('span', { class: 'grow' }),
        el('span', { class: 'meta mono nowrap', title: p.sessionId }, p.sessionId.slice(0, 8)),
      ),
    el(
      'div',
      { class: 'dbody transcript', id: 'sessions-body' },
      p ? transcript(p) : el('div', { class: 'review-empty fg-muted' }, 'Pick a session.'),
    ),
  );
}

/** The reader, for a session whose harness keeps a transcript to open. */
function readButton(p: SessionPreview): Child {
  return (
    p.readable &&
    el(
      'button',
      { class: 'btn sm', onclick: () => act('transcript') },
      'Open full transcript ',
      keycap('O'),
    )
  );
}

/** What the session under the cursor offers, named in full. */
function offer(p: SessionPreview | null): Child {
  if (!p) return null;
  if (p.offer === 'go') {
    return [
      readButton(p),
      el(
        'button',
        { class: 'btn sm primary', onclick: () => act('choose') },
        'Go to its pane ',
        el('span', { class: 'kbd on-accent' }, 'Enter'),
      ),
    ];
  }
  if (p.offer === 'resume') {
    return [
      el('span', { class: 'meta' }, 'runs'),
      el('code', { class: 'mono scommand ellipsis', title: p.command }, p.command),
      readButton(p),
      el(
        'button',
        { class: 'btn sm primary', onclick: () => act('resume') },
        'Resume in a new pane ',
        el('span', { class: 'kbd on-accent' }, 'R'),
      ),
    ];
  }
  return [
    p.why && el('span', { class: 'meta ellipsis' }, `Not resumable: ${p.why}`),
    readButton(p),
  ];
}

/**
 * Rust filters the rows; the field only says what it says. Its text is its
 * own once drawn: an echo that lands after the next letter would drop it.
 */
function searchField(search: string): HTMLElement {
  const live = document.getElementById('sessions-search');
  const field = el('input', {
    class: 'field filter',
    id: 'sessions-search',
    placeholder: 'Search sessions',
    'aria-label': 'Search sessions by title, harness, checkout or id',
    oninput: (e: Event) => {
      if (e.target instanceof HTMLInputElement) call('search_sessions', { search: e.target.value });
    },
    onkeydown: (e: KeyboardEvent) => {
      // The list has the keys back, filtered.
      if (actionFor(['filter'], e) === 'leave') {
        e.preventDefault();
        (e.target as HTMLElement).blur();
      }
    },
  });
  field.value = live instanceof HTMLInputElement ? live.value : search;
  return field;
}

/** A menu over what the listing holds; its first option is all of them. */
function menu(
  label: string,
  all: string,
  options: string[],
  chosen: string | null,
  pick: (value: string | null) => void,
): HTMLElement {
  const select = el(
    'select',
    {
      'aria-label': label,
      onchange: () => pick(select.value === '' ? null : select.value),
    },
    el('option', { value: '' }, all),
    options.map((o) => el('option', { value: o }, o)),
  );
  select.value = chosen ?? '';
  return el('label', { class: 'pick' }, select);
}

function filters(harness: string | null, checkout: string | null): void {
  call('filter_sessions', { harness, checkout });
}

/** The session whose transcript the body last showed, read. */
let drawnRead: string | null = null;

/** `/`: the search field takes the keys. */
export function focusSessionsSearch(): void {
  document.getElementById('sessions-search')?.focus();
}

export function drawSessions(): void {
  const s = state.sessions;
  const host = $('sessions');
  if (!s?.open) {
    host.hidden = true;
    host.replaceChildren();
    return;
  }
  const searchHad = document.activeElement?.id === 'sessions-search';
  const findHad = document.activeElement?.id === 'transcript-find';
  const scrolled = ['sessions-list', 'sessions-body', 'reader-body'].map(
    (id) => document.getElementById(id)?.scrollTop ?? 0,
  );
  host.hidden = false;
  if (s.reader) {
    host.replaceChildren(el('div', { class: 'scrim' }, drawReader(s.reader)));
    placeReader(s.reader, scrolled[2] ?? 0);
    drawnRead = null;
    // Redrawn as each letter arrives: the find keeps the keys and the caret.
    const field = document.getElementById('transcript-find');
    if (findHad && field instanceof HTMLInputElement) {
      field.focus();
      field.setSelectionRange(field.value.length, field.value.length);
    }
    return;
  }
  placeReader(null, 0);
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
          'aria-labelledby': 'sessions-title',
        },
        el(
          'div',
          { class: 'dhead review-head' },
          el(
            'div',
            { class: 'type-title', id: 'sessions-title' },
            'Recent sessions · ',
            s.repository,
          ),
          el('span', { class: 'meta' }, 'newest 100, every checkout'),
          el('span', { class: 'grow' }),
          searchField(s.search),
          menu('Harness', 'Every harness', s.harnesses, s.harness, (h) => filters(h, s.checkout)),
          menu('Checkout', 'Every checkout', s.checkouts, s.checkout, (c) => filters(s.harness, c)),
          el(
            'button',
            {
              class: 'icon',
              'aria-label': 'Close the recent sessions',
              onclick: () => act('closeLayer'),
            },
            icon('close'),
          ),
        ),
        el(
          'div',
          { class: 'review-split' },
          el(
            'aside',
            { class: 'history-list', id: 'sessions-list', 'aria-label': 'Sessions' },
            list(s),
          ),
          preview(s),
        ),
        el(
          'div',
          { class: 'dfoot meta' },
          keycap('J'),
          keycap('K'),
          ' move · ',
          keycap('/'),
          ' search · ',
          keycap('H'),
          keycap('C'),
          ' harness, checkout · ',
          keycap('O'),
          ' whole · ',
          keycap('Esc'),
          ' close',
          el('span', { class: 'grow' }),
          offer(s.preview),
        ),
      ),
    ),
  );
  ['sessions-list', 'sessions-body'].forEach((id, i) => {
    const node = document.getElementById(id);
    if (node) node.scrollTop = scrolled[i] ?? 0;
  });
  // A transcript newly read opens on how it ended, which is what a
  // preview is opened for; one redrawn stays where it was scrolled.
  const read = s.preview && !s.preview.loading ? s.preview.sessionId : null;
  const body = document.getElementById('sessions-body');
  if (body && read !== drawnRead) body.scrollTop = body.scrollHeight;
  drawnRead = read;
  host.querySelector('.hrow.on')?.scrollIntoView({ block: 'nearest' });
  // Redrawn as each letter arrives: the search keeps the keys and the caret.
  const field = document.getElementById('sessions-search');
  if (searchHad && field instanceof HTMLInputElement) {
    field.focus();
    field.setSelectionRange(field.value.length, field.value.length);
  }
}

export function sessionsOpen(): boolean {
  return state.sessions?.open === true;
}

/** The full transcript is open in place of the list, and holds its keys. */
export function readerOpen(): boolean {
  return sessionsOpen() && state.sessions?.reader != null;
}
