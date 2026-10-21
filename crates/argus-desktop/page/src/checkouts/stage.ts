// The Checkouts stage: the chosen repository's checkouts as a slim table,
// the branches without one folded under it, and the selected checkout said
// in full in a column beside it. Rust builds every row
// (`src/checkouts/table.rs`); the page draws them and reports what is
// chosen.

import type { Action } from '../bindings/Action';
import type { Checkouts } from '../bindings/Checkouts';
import type { Confirm } from '../bindings/Confirm';
import type { CheckoutsRow } from '../bindings/CheckoutsRow';
import type { LastCommit } from '../bindings/LastCommit';
import { startMenu, toggleStart } from '../chrome.js';
import {
  $,
  closeButton,
  el,
  escKey,
  focusQuietly,
  ghosts,
  icon,
  statusWord,
  type Child,
} from '../kit/dom.js';
import { closeLayer, openMenu } from '../kit/menu.js';
import { actionFor } from '../keys/keymap.js';
import { state } from '../state.js';
import { call } from '../connection/tauri.js';
import { afterRedraw, beforeRedraw, filesPanel } from './files.js';
import { branchBeside, stateOf, type Say } from './says.js';

function keycap(key: string): HTMLElement {
  return el('span', { class: 'kbd' }, key);
}

function empty(...body: Child[]): HTMLElement {
  return el('div', { class: 'empty' }, el('div', { class: 'empty-body' }, body));
}

/** The one filter field. It lives in the strip, which is rebuilt on every
 *  redraw, so the element is kept and handed back: typing redraws as each
 *  letter arrives, and a field made anew each time would lose the caret. */
let filterField: HTMLInputElement | null = null;

function filterInput(c: Checkouts): HTMLInputElement {
  if (!filterField) {
    const field = el('input', {
      class: 'field filter head-filter',
      id: 'checkouts-filter',
      placeholder: 'Filter checkouts and branches',
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
    filterField = field;
  }
  const field = filterField;
  // What Rust last heard, unless the person is mid-word: their text is newer.
  if (document.activeElement !== field && field.value !== c.filter) field.value = c.filter;
  return field;
}

/** What the strip carries on the Checkouts stage: the counts beside the
 *  title, and the filter, the primary action and the overflow. */
export function checkoutsStrip(): { meta: Child[]; tools: HTMLElement[] } | null {
  const c = state.checkouts;
  if (!c?.repository) return null;
  const branches = `${c.branches} other branch${c.branches === 1 ? '' : 'es'}`;
  const field = filterInput(c);
  const typing = document.activeElement === field;
  const caret = field.selectionStart ?? field.value.length;
  // Moving the field into a rebuilt strip drops its focus; give it back.
  if (typing)
    queueMicrotask(() => {
      if (!field.isConnected) return;
      field.focus();
      field.setSelectionRange(caret, caret);
    });
  return {
    meta: [
      el(
        'span',
        { class: 'meta head-meta', title: 'Enter open · D delete' },
        `${c.checkouts} checkout${c.checkouts === 1 ? '' : 's'} · ${branches}`,
        c.main && [' · primary ', el('span', { class: 'mono fg-ref' }, c.main)],
      ),
    ],
    // One primary and an overflow: Branches and Fetch all are the overflow's.
    tools: [
      field,
      el(
        'button',
        { class: 'btn primary', onclick: () => checkoutsAction('newWorktree') },
        'New worktree',
      ),
      el(
        'button',
        {
          class: 'icon',
          type: 'button',
          title: 'More: branches, fetch all',
          'aria-label': 'More about checkouts',
          'aria-haspopup': 'menu',
          onclick: (e: MouseEvent) => {
            const box = (e.currentTarget as HTMLElement).getBoundingClientRect();
            openMenu(
              'Checkouts',
              [
                {
                  label: c.branchesShown ? 'Hide other branches' : 'Show other branches',
                  hint: keycap('B'),
                  run: () => call('checkouts_action', { action: 'branches' }),
                },
                {
                  label: 'Fetch all',
                  hint: keycap('F'),
                  run: () => call('checkouts_action', { action: 'fetch' }),
                },
              ],
              { x: box.right, y: box.bottom + 4, align: 'end' },
            );
          },
        },
        icon('more'),
      ),
    ],
  };
}

/** The keys the table answers to, quietly under it. */
function keyBar(c: Checkouts): HTMLElement {
  const keys: [string, string][] = [
    ['Enter', 'open'],
    ['R', 'review'],
    ['H', 'history'],
    ['E', 'files'],
    ['P', 'pull'],
    ['N', 'new worktree'],
    ['D', 'delete'],
  ].filter(([key]) => key !== 'E' || c.filesOffered) as [string, string][];
  return el(
    'div',
    { class: 'kbar' },
    keys.map(([key, what]) => el('span', { class: 'kbar-key' }, keycap(key), ` ${what}`)),
  );
}

/** Puts the cursor on a row before an action that works on the cursor's
 *  row; a click on the row's own button must not wait for a second one. */
function onRow(row: CheckoutsRow, run: () => void): (e: MouseEvent) => void {
  return (e) => {
    e.stopPropagation();
    if (state.checkouts?.selected !== row.key) call('select_checkout_row', { key: row.key });
    run();
  };
}

/** What is done to a row, in the trailing column: Open and the overflow,
 *  quiet until the row is hovered or selected. Everything else a row can do
 *  is the overflow's, or the side column's. */
function actions(row: CheckoutsRow): Child {
  const checkout = row.checkout;
  const name = row.name ?? row.branch;
  const primary =
    checkout === null
      ? el(
          'button',
          {
            class: 'btn sm',
            onclick: onRow(row, () => checkoutsAction('newWorktree')),
          },
          'Check out',
        )
      : el(
          'button',
          {
            class: 'btn sm',
            onclick: onRow(row, () => call('checkouts_action', { action: 'choose' })),
          },
          'Open',
        );
  return el(
    'div',
    { class: 'ra' },
    primary,
    el(
      'button',
      {
        class: 'icon',
        title: 'More actions',
        'aria-label': `More actions for ${name}`,
        onclick: (e: MouseEvent) => {
          e.stopPropagation();
          const at = (e.currentTarget as HTMLElement).getBoundingClientRect();
          const choose = (run: () => void) => (): void => {
            if (state.checkouts?.selected !== row.key)
              call('select_checkout_row', { key: row.key });
            run();
          };
          const c = state.checkouts;
          const agents = checkout === null ? [] : state.rail.templates;
          openMenu(
            `More actions for ${name}`,
            [
              checkout !== null && {
                label: 'Review',
                run: () => call('open_review', { checkout }),
              },
              checkout !== null && {
                label: 'History',
                run: () => call('open_history', { checkout }),
              },
              checkout !== null && {
                label: 'Pull',
                run: choose(() => call('checkouts_action', { action: 'pull' })),
              },
              checkout !== null &&
                !!c?.filesOffered && {
                  label: 'Files',
                  hint: keycap('E'),
                  run: choose(() => call('checkouts_action', { action: 'files' })),
                },
              checkout !== null &&
                !!c?.reveal && {
                  label: c.reveal,
                  run: () => call('reveal_checkout', { checkout }),
                },
              checkout !== null && 'sep',
              checkout !== null && {
                label: 'New shell',
                run: () => call('spawn_shell', { checkout }),
              },
              ...agents.map((template) => ({
                label: `New agent · ${template}`,
                run: () => call('spawn_agent', { checkout: checkout as number, template }),
              })),
              checkout === null && {
                label: 'New worktree',
                run: choose(() => checkoutsAction('newWorktree')),
              },
              !row.primary && 'sep',
              !row.primary && {
                label: checkout === null ? 'Delete' : 'Remove',
                danger: true,
                run: choose(() => call('checkouts_action', { action: 'delete' })),
              },
            ],
            { x: at.right - 180, y: at.bottom + 4 },
          );
        },
      },
      '⋯',
    ),
  );
}

/** A State cell's words as markup; a quiet row yields none. */
function sayings(says: Say[], row: CheckoutsRow): Child[] {
  return says.map((say) => {
    switch (say.kind) {
      case 'detached':
        return el('span', { class: 'mono st-detached' }, 'detached');
      case 'changed':
        return el('span', { class: 'st-changed' }, say.text);
      case 'distance':
        return el('span', { class: 'mono fg-muted' }, say.text);
      case 'panes':
        return el(
          'span',
          { class: 'st-panes' },
          row.running.map((pane) =>
            el('span', {
              class: `sdot ${pane.status}`,
              title: `${pane.title}: ${statusWord(pane.status)}`,
            }),
          ),
          `${say.count} pane${say.count === 1 ? '' : 's'}`,
        );
      case 'feature':
        return el(
          'span',
          { class: `mono${say.viewed ? ' fg-accent' : ' fg-muted'}` },
          `#${say.number}`,
        );
    }
  });
}

function tableRow(row: CheckoutsRow, selected: boolean): HTMLElement {
  const beside = branchBeside(row);
  return el(
    'tr',
    {
      class: selected ? 'on' : null,
      title: row.last ? row.last.summary : null,
      'aria-selected': String(selected),
      onclick: () => call('select_checkout_row', { key: row.key }),
      ondblclick: () => call('checkouts_action', { action: 'choose' }),
    },
    el(
      'td',
      { class: 'col-name' },
      row.name !== null
        ? [
            el('span', { class: 'cname' }, row.name),
            row.primary && el('span', { class: 'ctag' }, 'primary'),
            beside && el('span', { class: 'cbranch mono' }, beside),
          ]
        : // A branch with no directory is named where a checkout is, so the
          // first column is never blank.
          el('span', { class: 'mono fg-ref bname' }, icon('branch'), row.branch),
    ),
    el(
      'td',
      { class: 'col-state' },
      row.name !== null
        ? sayings(stateOf(row), row)
        : [
            row.git !== null && el('span', { class: 'mono fg-muted' }, row.git),
            row.note && el('span', { class: 'meta' }, row.note),
            // A feature's branch stays listed with the rest folded; its number says why.
            row.feature !== null &&
              sayings([{ kind: 'feature', number: row.feature, viewed: row.viewed }], row),
          ],
    ),
    el('td', { class: 'col-last' }, row.last && commitLine(row.last, false)),
    el('td', { class: 'row-actions' }, actions(row)),
  );
}

function commitLine(commit: LastCommit, ago = true): Child {
  return [
    el('span', { class: 'mono fg-faint' }, commit.short),
    ' ',
    el('span', { class: 'fg-muted' }, commit.summary),
    ago && el('span', { class: 'fg-faint' }, ` · ${commit.ago}`),
  ];
}

/** The row that stands for the branches without a checkout: they are a
 *  count until it is opened, then the rows beneath it. */
function branchesFold(c: Checkouts): HTMLElement {
  const label = `${c.branches} branch${c.branches === 1 ? '' : 'es'} without a checkout`;
  return el(
    'tr',
    { class: 'fold' },
    el(
      'td',
      { colspan: '4' },
      el(
        'button',
        {
          class: 'fold-btn',
          type: 'button',
          'aria-expanded': String(c.branchesShown),
          title: 'B',
          onclick: () => call('checkouts_action', { action: 'branches' }),
        },
        icon(c.branchesShown ? 'down' : 'right'),
        label,
      ),
    ),
  );
}

function table(c: Checkouts): HTMLElement {
  const checkouts = c.rows.filter((row) => row.checkout !== null);
  const branches = c.rows.filter((row) => row.checkout === null);
  const rows = (list: CheckoutsRow[]): HTMLElement[] =>
    list.map((row) => tableRow(row, row.key === c.selected));
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
          el('th', { class: 'col-name' }, 'Checkout'),
          el('th', { class: 'col-state' }, 'State'),
          el('th', { class: 'col-last' }, 'Last commit'),
          el('th', { class: 'col-actions' }, el('span', { class: 'sr-only' }, 'Actions')),
        ),
      ),
      el('tbody', null, rows(checkouts), c.branches > 0 && branchesFold(c), rows(branches)),
    ),
  );
}

// ---- The selected checkout, beside the table -------------------------------

function act(label: string, key: string, run: () => void): HTMLElement {
  return el('button', { class: 'btn sm', onclick: run }, label, ' ', keycap(key));
}

function fact(label: string, value: Child): HTMLElement[] {
  return [el('dt', null, label), el('dd', null, value)];
}

/** A worktree's path from the repository's own `.argus/` folder when it
 *  lives under one, whole otherwise; the absolute path stays in the tooltip. */
export function repoRelative(path: string): string {
  const flat = path.replaceAll('\\', '/');
  const at = flat.indexOf('/.argus/');
  return at >= 0 ? flat.slice(at + 1) : path;
}

/** The column for the selected row: where it is, what it tracks, what is
 *  running on it and what can be done to it. It follows the selection, and
 *  every button is a key the table answers to as well. */
function sideColumn(c: Checkouts, row: CheckoutsRow): HTMLElement {
  const d = c.detail;
  const checkout = row.checkout;
  const name = row.name ?? row.branch;
  const hot =
    checkout !== null && state.start?.checkout === checkout && state.start.from === 'table';
  return el(
    'aside',
    { class: 'cside', id: 'cside', 'aria-label': `${name}, selected` },
    el(
      'div',
      null,
      el(
        'div',
        { class: 'cs-title' },
        el('h2', { class: row.name === null ? 'mono' : '' }, name),
        row.primary && el('span', { class: 'ctag' }, 'primary'),
      ),
      d && el('div', { class: 'cs-path mono', title: d.path }, repoRelative(d.path)),
      row.note && el('div', { class: 'meta' }, row.note),
    ),
    el(
      'div',
      { class: 'cs-acts' },
      checkout !== null && [
        el(
          'button',
          { class: 'btn sm', onclick: () => call('checkouts_action', { action: 'choose' }) },
          'Open',
        ),
        act('Review', 'R', () => call('open_review', { checkout })),
        act('History', 'H', () => call('open_history', { checkout })),
        act('Pull', 'P', () => call('checkouts_action', { action: 'pull' })),
      ],
      checkout === null && act('Check out', 'N', () => checkoutsAction('newWorktree')),
    ),
    d &&
      el(
        'dl',
        { class: 'cs-facts' },
        d.upstream && fact('Upstream', d.upstream),
        d.changes && fact('Changes', d.changes),
        row.feature !== null &&
          fact('Feature', [
            el('span', { class: `mono${row.viewed ? ' fg-accent' : ''}` }, `#${row.feature}`),
            d.featureTitle && ` ${d.featureTitle}`,
          ]),
        row.last && fact('Last commit', commitLine(row.last)),
      ),
    row.running.length > 0 &&
      el(
        'div',
        { class: 'cs-running' },
        el(
          'div',
          { class: 'mlabel' },
          row.shared !== null ? `Running here · shared by ${row.shared}` : 'Running here',
        ),
        row.running.map((pane) =>
          el(
            'button',
            {
              class: 'cs-pane',
              type: 'button',
              title: pane.agent ? 'an agent' : 'a shell',
              onclick: () => call('go_to_pane', { pane: pane.id }),
            },
            el('span', { class: `sdot ${pane.status}` }),
            el('span', { class: 'cut grow' }, pane.title),
            el('span', { class: `cs-word ${pane.status}` }, statusWord(pane.status)),
          ),
        ),
      ),
    el('div', { class: 'grow' }),
    el(
      'div',
      { class: 'cs-foot' },
      checkout !== null &&
        c.filesOffered &&
        el(
          'button',
          {
            class: 'btn sm cs-quiet',
            onclick: () => call('checkouts_action', { action: 'files' }),
          },
          icon('folder-sm'),
          'Files',
          ' ',
          keycap('E'),
        ),
      checkout !== null &&
        c.reveal &&
        el(
          'button',
          { class: 'btn sm cs-quiet', onclick: () => call('reveal_checkout', { checkout }) },
          icon('external'),
          c.reveal,
        ),
      checkout !== null &&
        el(
          'span',
          { class: 'start-at' },
          el(
            'button',
            {
              class: `btn sm cs-quiet${hot ? ' hot' : ''}`,
              'aria-expanded': String(hot),
              title: 'Start a shell or an agent',
              onclick: (e: MouseEvent) => {
                e.stopPropagation();
                toggleStart(checkout, 'table');
              },
            },
            icon('plus'),
            'New pane',
          ),
          hot && startMenu(name, checkout),
        ),
      el('span', { class: 'grow' }),
      !row.primary &&
        el(
          'button',
          {
            class: 'btn sm danger-line',
            onclick: () => call('checkouts_action', { action: 'delete' }),
          },
          checkout === null ? 'Delete' : 'Remove',
          ' ',
          keycap('D'),
        ),
    ),
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
          class: 'dialog',
          role: 'dialog',
          'aria-modal': 'true',
          'aria-labelledby': 'worktree-title',
        },
        el(
          'div',
          { class: 'dhead' },
          el('div', { class: 'type-title grow', id: 'worktree-title' }, 'New worktree'),
          closeButton(closeLayer),
        ),
        el(
          'div',
          { class: 'dbody prose' },
          el('label', { class: 'label', for: 'worktree-name' }, 'Branch'),
          name,
        ),
        el(
          'div',
          { class: 'dfoot' },
          el('span', { class: 'dwhy grow' }, 'Off ', el('span', { class: 'mono' }, from)),
          el('button', { class: 'btn', onclick: closeLayer }, 'Cancel ', escKey()),
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
        el(
          'div',
          { class: 'warn-callout' },
          icon('warn'),
          el('span', null, 'It has commits no other branch has, and nothing is pushed.'),
        ),
        question.commits.length > 0 &&
          el(
            'div',
            { class: 'commits' },
            question.commits.map((commit) => el('div', null, commitLine(commit))),
          ),
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
      el(
        'div',
        { class: 'dhead' },
        el('span', { class: 'danger-tile' }, icon('trash')),
        el('div', { class: 'type-title grow', id: 'confirm-title' }, title),
        closeButton(answerNo),
      ),
      el('div', { class: 'dbody prose' }, body),
      el(
        'div',
        { class: 'dfoot' },
        el('span', { class: 'grow' }),
        el(
          'button',
          { class: 'btn', id: 'confirm-keep', onclick: answerNo },
          force ? 'Keep the branch ' : 'Keep it ',
          escKey(),
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
  focusQuietly(document.getElementById('confirm-keep'));
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
  const body = c.rows.length
    ? table(c)
    : empty(
        el(
          'div',
          { class: 'fg-muted' },
          c.filter ? `Nothing matches “${c.filter}”.` : 'No checkouts.',
        ),
      );
  const before = beforeRedraw();
  const row = c.rows.find((r) => r.key === c.selected);
  // The files panel takes the side column's place: both are about the
  // selected checkout, and the table keeps its room.
  const side = c.files ? filesPanel(c.files) : row && sideColumn(c, row);
  stage.classList.add('with-side');
  stage.replaceChildren(el('div', { class: 'cmain' }, body, keyBar(c)), ...(side ? [side] : []));
  afterRedraw(before);
  drawConfirm(c);
  stage.querySelector('tr.on')?.scrollIntoView({ block: 'nearest' });
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
