// The Checkouts stage: the chosen repository's checkouts as an operational
// table, the branches without one beneath them while shown, and the
// selected checkout said in full along the foot. Rust builds every row
// (`src/checkouts/table.rs`); the page draws them and reports what is
// chosen.

import type { Action } from '../bindings/Action';
import type { Checkouts } from '../bindings/Checkouts';
import type { Confirm } from '../bindings/Confirm';
import type { CheckoutsRow } from '../bindings/CheckoutsRow';
import type { LastCommit } from '../bindings/LastCommit';
import { startMenu, toggleStart } from '../chrome.js';
import { $, el, ghosts, icon, statusPill, type Child } from '../kit/dom.js';
import { closeLayer } from '../kit/menu.js';
import { actionFor } from '../keys/keymap.js';
import { state } from '../state.js';
import { call } from '../connection/tauri.js';

function keycap(key: string): HTMLElement {
  return el('span', { class: 'kbd' }, key);
}

function empty(...body: Child[]): HTMLElement {
  return el('div', { class: 'empty' }, el('div', { class: 'empty-body' }, body));
}

function head(c: Checkouts): HTMLElement {
  const filter = el('input', {
    class: 'field filter',
    id: 'checkouts-filter',
    placeholder: 'Filter branches',
    'aria-label': 'Filter checkouts and branches',
    oninput: (e: Event) => {
      if (e.target instanceof HTMLInputElement)
        call('filter_checkouts', { filter: e.target.value });
    },
    onkeydown: (e: KeyboardEvent) => {
      if (actionFor(['filter'], e) === 'leave') {
        e.preventDefault();
        (e.target as HTMLElement).blur();
      }
    },
  });
  filter.value = c.filter;
  const branches = `${c.branches} other branch${c.branches === 1 ? '' : 'es'}`;
  return el(
    'div',
    { class: 'fhead index-head' },
    el(
      'div',
      { class: 'actions index-tools' },
      el(
        'span',
        { class: 'meta' },
        `${c.checkouts} checkout${c.checkouts === 1 ? '' : 's'} · ${branches}`,
        c.main && [' · main ', el('span', { class: 'mono fg-ref' }, c.main)],
      ),
      el('span', { class: 'grow' }),
      filter,
      el(
        'button',
        {
          class: 'btn',
          'aria-pressed': String(c.branchesShown),
          onclick: () => call('checkouts_action', { action: 'branches' }),
        },
        'Branches ',
        keycap('B'),
      ),
      el(
        'button',
        { class: 'btn', onclick: () => call('checkouts_action', { action: 'fetch' }) },
        'Fetch all ',
        keycap('F'),
      ),
      el(
        'button',
        { class: 'btn primary', onclick: () => checkoutsAction('newWorktree') },
        'New worktree',
      ),
    ),
  );
}

/** The keys the table answers to, quietly under it. */
function keyBar(): HTMLElement {
  const keys: [string, string][] = [
    ['Enter', 'open'],
    ['R', 'review'],
    ['H', 'history'],
    ['P', 'pull'],
    ['N', 'new worktree'],
    ['D', 'delete'],
  ];
  return el(
    'div',
    { class: 'kbar' },
    keys.map(([key, what]) => el('span', { class: 'kbar-key' }, keycap(key), ` ${what}`)),
  );
}

function running(row: CheckoutsRow): Child {
  if (!row.running.length) return null;
  return [
    row.shared !== null &&
      el('span', { class: 'shared' }, icon('people'), ` shared by ${row.shared} `),
    row.running.map((pane) =>
      el(
        'span',
        { class: 'pane-status', title: pane.agent ? 'an agent' : 'a shell' },
        statusPill(pane.status),
      ),
    ),
  ];
}

/** What is done to the selected row, in the trailing column: the one thing
 *  most wanted, and the start menu. The rest sit in the detail strip under
 *  the row, so nothing is drawn over a cell. */
function actions(row: CheckoutsRow): Child {
  const open = el(
    'button',
    { class: 'btn sm', onclick: () => call('checkouts_action', { action: 'choose' }) },
    'Open',
  );
  if (row.checkout === null) return open;
  const checkout = row.checkout;
  const hot = state.start?.checkout === checkout && state.start.from === 'table';
  return [
    open,
    el(
      'span',
      { class: 'start-at' },
      el(
        'button',
        {
          class: `icon${hot ? ' hot' : ''}`,
          title: 'Start a shell or an agent',
          'aria-label': `Start a shell or an agent on ${row.name ?? row.branch}`,
          'aria-expanded': String(hot),
          onclick: (e: MouseEvent) => {
            e.stopPropagation();
            toggleStart(checkout, 'table');
          },
        },
        icon('plus'),
      ),
      hot && startMenu(row.name ?? row.branch, checkout),
    ),
  ];
}

function tableRow(row: CheckoutsRow, selected: boolean): HTMLElement {
  return el(
    'tr',
    {
      class: selected ? 'on' : null,
      'aria-selected': String(selected),
      onclick: () => call('select_checkout_row', { key: row.key }),
      ondblclick: () => call('checkouts_action', { action: 'choose' }),
    },
    el(
      'td',
      null,
      row.name !== null && [
        el('b', null, row.name),
        ' ',
        el('span', { class: 'meta' }, row.primary ? 'primary' : 'worktree'),
      ],
    ),
    el('td', { class: 'mono fg-ref' }, row.branch),
    el(
      'td',
      null,
      row.git !== null && el('span', { class: `git${row.clean ? ' clean' : ''}` }, row.git),
      row.note && el('span', { class: 'meta' }, row.note),
    ),
    el('td', { class: 'col-running' }, running(row)),
    el(
      'td',
      { class: `col-feature mono${row.viewed ? ' fg-accent' : ''}` },
      row.feature !== null && `#${row.feature}`,
    ),
    el('td', { class: 'col-last' }, row.last && commitLine(row.last)),
    el('td', { class: 'row-actions' }, selected && actions(row)),
  );
}

function commitLine(commit: LastCommit): Child {
  return [
    el('span', { class: 'mono fg-faint' }, commit.short),
    ' ',
    el('span', { class: 'fg-muted' }, commit.summary),
    el('span', { class: 'fg-faint' }, ` · ${commit.ago}`),
  ];
}

function act(label: string, key: string, run: () => void, danger = false): HTMLElement {
  return el(
    'button',
    { class: `btn sm${danger ? ' danger-line' : ''}`, onclick: run },
    label,
    ' ',
    keycap(key),
  );
}

/** The selected row's own strip, directly under it: where it is, what it
 *  tracks, what changed, and what can be done to it. */
function detailRow(c: Checkouts, row: CheckoutsRow): HTMLElement | null {
  const d = c.detail;
  if (!d) return null;
  const checkout = row.checkout;
  return el(
    'tr',
    { class: 'detail-row' },
    el(
      'td',
      { colspan: '7' },
      el(
        'div',
        { class: 'cdetail' },
        el(
          'div',
          { class: 'cfacts' },
          stack('Path', el('span', { class: 'mono' }, d.path)),
          d.upstream && stack('Upstream', d.upstream),
          d.changes && stack('Changes', d.changes),
        ),
        el('span', { class: 'grow' }),
        el(
          'div',
          { class: 'cacts' },
          checkout !== null && [
            act('Review', 'R', () => call('open_review', { checkout })),
            act('History', 'H', () => call('open_history', { checkout })),
            act('Pull', 'P', () => call('checkouts_action', { action: 'pull' })),
          ],
          checkout === null && act('New worktree', 'N', () => checkoutsAction('newWorktree')),
          !row.primary &&
            act(
              checkout === null ? 'Delete' : 'Remove',
              'D',
              () => call('checkouts_action', { action: 'delete' }),
              true,
            ),
        ),
      ),
    ),
  );
}

function group(label: string): HTMLElement {
  return el('tr', { class: 'group' }, el('td', { colspan: '7' }, label));
}

function table(c: Checkouts): HTMLElement {
  const checkouts = c.rows.filter((row) => row.checkout !== null);
  const branches = c.rows.filter((row) => row.checkout === null);
  const rows = (list: CheckoutsRow[]): (HTMLElement | null)[] =>
    list.flatMap((row) => {
      const selected = row.key === c.selected;
      return [tableRow(row, selected), selected ? detailRow(c, row) : null];
    });
  return el(
    'div',
    { class: 'ctable-wrap' },
    el(
      'table',
      { class: 'ctable', 'aria-label': `Checkouts in ${c.repository}` },
      el(
        'thead',
        null,
        el(
          'tr',
          null,
          ['Checkout', 'Branch', 'Git', 'Running', 'Feature', 'Last commit'].map((name) =>
            el('th', { class: `col-${name.split(' ')[0]?.toLowerCase()}` }, name),
          ),
          el('th', null, el('span', { class: 'sr-only' }, 'Actions')),
        ),
      ),
      el(
        'tbody',
        null,
        checkouts.length > 0 && [group('Checkouts'), rows(checkouts)],
        branches.length > 0 && [group('Branches without a checkout'), rows(branches)],
      ),
    ),
  );
}

function stack(label: string, value: Child): HTMLElement {
  return el(
    'div',
    { class: 'stack-2' },
    el('span', { class: 'mlabel' }, label),
    el('span', { class: 'meta' }, value),
  );
}

// ---- New worktree and the questions before deleting ----------------------

/** A new branch off the selected checkout, in a worktree of its own. */
function nameDialog(from: string): void {
  const name = el('input', {
    class: 'field mono',
    id: 'worktree-name',
    placeholder: 'desktop-checkouts',
  });
  const make = (): void => {
    call('new_worktree', { name: name.value });
    closeLayer();
  };
  name.addEventListener('keydown', (e) => {
    const action = actionFor(['writing'], e);
    if (action === 'choose') make();
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
          'aria-labelledby': 'worktree-title',
        },
        el(
          'div',
          { class: 'dhead' },
          el('div', { class: 'type-title', id: 'worktree-title' }, 'New worktree'),
        ),
        el(
          'div',
          { class: 'dbody' },
          el('label', { class: 'label', for: 'worktree-name' }, 'Branch'),
          name,
          el('div', { class: 'meta' }, 'Off ', el('span', { class: 'mono' }, from), '.'),
        ),
        el(
          'div',
          { class: 'dfoot' },
          el('span', { class: 'grow' }),
          el('button', { class: 'btn', onclick: closeLayer }, 'Cancel'),
          el('button', { class: 'btn primary', onclick: make }, 'Create'),
        ),
      ),
    ),
  );
  name.focus();
}

/** Says no to the stage's question, as Escape or a click outside does. */
export function answerNo(): boolean {
  if (!state.checkouts?.confirm) return false;
  call('answer_checkouts', { yes: false });
  closeLayer();
  return true;
}

function confirmDialog(question: Confirm): HTMLElement {
  const force = question.kind === 'force';
  const title = force
    ? ['Delete ', el('span', { class: 'mono' }, question.branch), ' anyway?']
    : ['Remove ', el('span', { class: 'mono' }, question.name), '?'];
  const body = force
    ? [
        el('div', { class: 'fg-muted' }, 'It has commits no other branch has.'),
        question.commits.length > 0 &&
          el(
            'div',
            { class: 'commits' },
            question.commits.map((commit) => el('div', null, commitLine(commit))),
          ),
        el('div', { class: 'meta' }, 'Local only: nothing is pushed.'),
      ]
    : [el('div', { class: 'fg-muted' }, 'Its panes end; its directory and branch are deleted.')];
  return el(
    'div',
    {
      class: 'scrim',
      id: 'checkouts-confirm',
      onclick: (e: MouseEvent) => {
        if (e.target === e.currentTarget) answerNo();
      },
    },
    el(
      'section',
      {
        class: 'dialog confirm-dialog',
        role: 'alertdialog',
        'aria-modal': 'true',
        'aria-labelledby': 'confirm-title',
      },
      el('div', { class: 'dhead' }, el('div', { class: 'type-title', id: 'confirm-title' }, title)),
      el('div', { class: 'dbody' }, body),
      el(
        'div',
        { class: 'dfoot' },
        el('span', { class: 'grow' }),
        el(
          'button',
          { class: 'btn', id: 'confirm-keep', onclick: answerNo },
          force ? 'Keep the branch' : 'Keep it',
        ),
        el(
          'button',
          {
            class: 'btn danger',
            onclick: () => {
              call('answer_checkouts', { yes: true });
              closeLayer();
            },
          },
          force ? 'Delete anyway' : 'Remove',
        ),
      ),
    ),
  );
}

/** The stage's question over everything, while there is one. */
function drawConfirm(c: Checkouts): void {
  const layer = $('layer');
  const shown = document.getElementById('checkouts-confirm');
  if (!c.confirm) {
    if (shown) closeLayer();
    return;
  }
  if (shown) return;
  layer.replaceChildren(confirmDialog(c.confirm));
  document.getElementById('confirm-keep')?.focus();
}

export function drawCheckouts(): void {
  if (state.rail.stage !== 'checkouts') return;
  const stage = $('cstage');
  const c = state.checkouts;
  if (!c) {
    stage.replaceChildren(ghosts([[0.3], [0.9], [0.85], [0.9], [0.6]]));
    return;
  }
  if (!c.repository) {
    stage.replaceChildren(
      empty(
        icon('folder'),
        el('div', { class: 'type-title' }, 'No repository'),
        el('div', { class: 'fg-muted' }, 'Choose one on the left.'),
      ),
    );
    return;
  }
  const filterHad = document.activeElement?.id === 'checkouts-filter';
  const body = c.rows.length
    ? table(c)
    : empty(
        el(
          'div',
          { class: 'fg-muted' },
          c.filter ? `Nothing matches “${c.filter}”.` : 'No checkouts.',
        ),
      );
  stage.replaceChildren(head(c), body, keyBar());
  drawConfirm(c);
  stage.querySelector('tr.on')?.scrollIntoView({ block: 'nearest' });
  // Redrawn as each letter arrives: the filter keeps the keys and the caret.
  if (filterHad) {
    const again = document.getElementById('checkouts-filter');
    if (again instanceof HTMLInputElement) {
      again.focus();
      again.setSelectionRange(again.value.length, again.value.length);
    }
  }
}

/** Runs a Checkouts key, saying whether it took it. */
export function checkoutsAction(action: Action): boolean {
  if (action === 'filter') {
    $('checkouts-filter').focus();
    return true;
  }
  if (action === 'newWorktree') {
    const row = state.checkouts?.rows.find((r) => r.key === state.checkouts?.selected);
    if (row && row.checkout !== null) nameDialog(row.branch || (row.name ?? ''));
    else call('new_worktree', { name: null });
    return true;
  }
  call('checkouts_action', { action });
  return true;
}
