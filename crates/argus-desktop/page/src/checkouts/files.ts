// The files panel beside the Checkouts table: one checkout's folder as it is
// on disk, a level at a time. Rust reads the folders and flattens the tree
// (`src/checkouts/files_page.rs`); the page draws the rows it is sent, says
// what is clicked, and keeps its own focus, which a redraw must not lose.

import type { FileRow } from '../bindings/FileRow';
import type { FilesGesture } from '../bindings/FilesGesture';
import type { FilesPage } from '../bindings/FilesPage';
import type { Mode } from '../bindings/Mode';
import { actionFor } from '../keys/keymap.js';
import { el, ghosts, icon } from '../kit/dom.js';
import { contextMenu, type MenuItem } from '../kit/menu.js';
import { call } from '../connection/tauri.js';
import { state } from '../state.js';

function gesture(g: FilesGesture): void {
  call('files', { gesture: g });
}

/** The checkout whose files were last drawn: a panel opening on another
 *  takes the keys, one redrawn for a listing does not. */
let drawnFor: number | null = null;
/** Whether the next draw gives the tree the keys: after a click in the panel. */
let keepKeys = false;

const filesOf = (): FilesPage | null => state.checkouts?.files ?? null;

/** Whether the keys are in the panel's tree rather than a field. */
export function filesFocused(): boolean {
  const panel = document.getElementById('cfiles');
  const at = document.activeElement;
  return (
    !!panel && !!at && panel.contains(at) && !(at instanceof HTMLInputElement) && filesOf() !== null
  );
}

/** Escape: puts the panel away if it has the keys. */
export function closeFilesIfFocused(): boolean {
  if (!filesFocused()) return false;
  gesture({ on: 'close' });
  return true;
}

/** `/`: the filter field takes the keys. */
export function focusFilesFilter(): void {
  document.getElementById('cfiles-filter')?.focus();
}

function keycap(key: string): HTMLElement {
  return el('span', { class: 'kbd' }, key);
}

function pathOf(files: FilesPage, relative: string): string {
  const tail = relative.split('/').join(files.separator);
  return `${files.root.replace(/[\\/]+$/, '')}${files.separator}${tail}`;
}

function copy(text: string): void {
  call('copy_text', { text });
}

function menuFor(files: FilesPage, row: FileRow): (MenuItem | false)[] {
  return [
    !row.dir && {
      label: 'Open in Code',
      hint: keycap('Enter'),
      run: () => gesture({ on: 'open', path: row.path, how: 'code' }),
    },
    !row.dir && {
      label: 'Open in the editor',
      hint: keycap('O'),
      run: () => gesture({ on: 'open', path: row.path, how: 'editor' }),
    },
    files.canReveal && {
      label: files.reveal,
      run: () => gesture({ on: 'reveal', path: row.path }),
    },
    { label: 'Copy path', run: () => copy(pathOf(files, row.path)) },
    { label: 'Copy relative path', run: () => copy(row.path) },
    'sep',
    { label: 'Open a shell here', run: () => gesture({ on: 'shell' }) },
  ];
}

function mark(row: FileRow): HTMLElement | null {
  if (row.mark === 'changed')
    return el('span', { class: 'cf-mark changed', title: 'Changed', 'aria-label': 'changed' }, 'M');
  if (row.mark === 'untracked')
    return el(
      'span',
      { class: 'cf-mark untracked', title: 'Untracked', 'aria-label': 'untracked' },
      '?',
    );
  if (row.mark === 'ignored') return el('span', { class: 'cf-mark ignored' }, 'ignored');
  return null;
}

function treeRow(files: FilesPage, row: FileRow, at: number): HTMLElement {
  const on = row.path === files.selected;
  const buttons =
    on && !row.dir
      ? el(
          'span',
          { class: 'cf-acts' },
          el(
            'button',
            {
              class: 'icon sm',
              type: 'button',
              title: 'Open in Code',
              'aria-label': `Open ${row.name} in Code`,
              tabindex: '-1',
              onclick: (e: MouseEvent) => {
                e.stopPropagation();
                gesture({ on: 'open', path: row.path, how: 'code' });
              },
            },
            icon('code-sm'),
          ),
          el(
            'button',
            {
              class: 'icon sm',
              type: 'button',
              title: 'Open in the editor',
              'aria-label': `Open ${row.name} in the editor`,
              tabindex: '-1',
              onclick: (e: MouseEvent) => {
                e.stopPropagation();
                gesture({ on: 'open', path: row.path, how: 'editor' });
              },
            },
            icon('external'),
          ),
        )
      : null;
  return el(
    'div',
    {
      class: `cf-row${on ? ' on' : ''}${row.mark === 'ignored' ? ' ignored' : ''}${row.dir ? ' dir' : ''}`,
      id: `cf-r${at}`,
      role: 'treeitem',
      'aria-level': String(row.depth + 1),
      'aria-selected': String(on),
      'aria-expanded': row.dir ? String(row.open) : null,
      title: row.path,
      style: `--d: ${row.depth}`,
      onclick: () => gesture({ on: 'click', path: row.path }),
      ondblclick: () => {
        if (!row.dir) gesture({ on: 'open', path: row.path, how: 'code' });
      },
      oncontextmenu: (e: MouseEvent) => {
        gesture({ on: 'select', path: row.path });
        contextMenu(`${row.name}`, () => menuFor(files, row))(e);
      },
    },
    el('span', { class: `cf-chev${row.open ? ' open' : ''}` }, row.dir ? icon('right') : null),
    el('span', { class: 'cf-glyph' }, icon(row.dir ? 'folder-sm' : 'file-sm')),
    el('span', { class: 'cf-name' }, row.name),
    buttons,
    mark(row),
  );
}

function head(files: FilesPage): HTMLElement {
  return el(
    'div',
    { class: 'cf-head' },
    el(
      'div',
      { class: 'cf-title' },
      el('span', { class: 'cf-kicker' }, 'Files'),
      el('span', { class: 'cf-checkout mono', title: files.root }, files.name),
    ),
    files.canReveal &&
      el(
        'button',
        {
          class: 'btn sm',
          type: 'button',
          title: `${files.reveal}: the checkout's folder`,
          onclick: () => gesture({ on: 'reveal', path: null }),
        },
        icon('folder-sm'),
        files.reveal,
      ),
    el(
      'button',
      {
        class: 'icon',
        type: 'button',
        title: 'Open a shell here',
        'aria-label': 'Open a shell here',
        onclick: () => gesture({ on: 'shell' }),
      },
      icon('console-sm'),
    ),
    el(
      'button',
      {
        class: 'icon',
        type: 'button',
        title: 'Read the folder again',
        'aria-label': 'Read the folder again',
        onclick: () => gesture({ on: 'reread' }),
      },
      icon('refresh'),
    ),
    el(
      'button',
      {
        class: 'icon',
        type: 'button',
        title: 'Close the files (Esc)',
        'aria-label': 'Close the files',
        onclick: () => gesture({ on: 'close' }),
      },
      icon('close'),
    ),
  );
}

function tools(files: FilesPage): HTMLElement {
  const filter = el('input', {
    class: 'field cf-filter',
    id: 'cfiles-filter',
    placeholder: 'Filter files',
    'aria-label': 'Filter the files shown',
    autocomplete: 'off',
    spellcheck: 'false',
    oninput: (e: Event) => {
      if (e.target instanceof HTMLInputElement) gesture({ on: 'filter', filter: e.target.value });
    },
    onkeydown: (e: KeyboardEvent) => {
      if (actionFor(['filter'], e) === 'leave') {
        e.preventDefault();
        keepKeys = true;
        document.getElementById('cfiles-tree')?.focus();
      }
    },
  });
  filter.value = files.filter;
  const mode = (value: Mode, label: string, count: number | null): HTMLElement =>
    el(
      'button',
      {
        class: `segb${files.mode === value ? ' on' : ''}`,
        type: 'button',
        'aria-pressed': String(files.mode === value),
        onclick: () => gesture({ on: 'mode', mode: value }),
      },
      label,
      count !== null && count > 0 && el('span', { class: 'cf-count' }, String(count)),
    );
  return el(
    'div',
    { class: 'cf-tools' },
    el(
      'label',
      { class: 'cf-search' },
      icon('search'),
      filter,
      el('span', { class: 'kbd cf-slash', 'aria-hidden': 'true' }, '/'),
    ),
    el(
      'div',
      { class: 'seg', role: 'group', 'aria-label': 'Which files to list' },
      mode('all', 'All', null),
      mode('changed', 'Changed', files.changed),
    ),
  );
}

function crumb(files: FilesPage): HTMLElement {
  const last = files.crumb.length - 1;
  return el(
    'div',
    { class: 'cf-crumb mono', 'aria-label': 'Folder' },
    files.crumb.flatMap((part, i) => [
      i > 0 && el('span', { class: 'cf-sep' }, '/'),
      el('span', { class: i === last ? 'cf-last' : null }, part),
    ]),
  );
}

function body(files: FilesPage): HTMLElement {
  if (files.error) {
    return el(
      'div',
      { class: 'cf-note warn' },
      icon('warn'),
      el('span', null, `Could not read the folder: ${files.error}`),
    );
  }
  if (files.loading) return ghosts([[0.5], [0.7], [0.4], [0.6], [0.55]]);
  if (files.rows.length === 0) {
    const text = files.filter
      ? `Nothing named “${files.filter}” in the folders read.`
      : files.mode === 'changed'
        ? 'Nothing changed in the folders read.'
        : 'This folder is empty.';
    return el('div', { class: 'cf-note' }, text);
  }
  return el(
    'div',
    { class: 'cf-rows' },
    files.rows.map((row, i) => treeRow(files, row, i)),
  );
}

function foot(files: FilesPage): HTMLElement {
  const f = files.footer;
  if (!f) {
    return el('div', { class: 'cf-foot' }, el('span', { class: 'cf-hint' }, 'Select a file.'));
  }
  return el(
    'div',
    { class: 'cf-foot' },
    el('span', { class: 'cf-glyph' }, icon(f.dir ? 'folder-sm' : 'file-sm')),
    el('span', { class: 'cf-fname mono', title: f.name }, f.name),
    el('span', { class: 'cf-detail' }, f.detail),
    !f.dir &&
      files.selected !== null &&
      el(
        'button',
        {
          class: 'btn sm',
          type: 'button',
          onclick: () => {
            if (files.selected !== null) gesture({ on: 'open', path: files.selected, how: 'code' });
          },
        },
        'Open in Code ',
        keycap('Enter'),
      ),
  );
}

/** The panel for `files`, built for the stage to place beside the table.
 *  The tree's scroll and the keys are the caller's to keep across the redraw. */
export function filesPanel(files: FilesPage): HTMLElement {
  const tree = el(
    'div',
    {
      class: 'cf-tree',
      id: 'cfiles-tree',
      role: 'tree',
      tabindex: '0',
      'aria-label': `Files of ${files.name}`,
      onmousedown: () => {
        keepKeys = true;
      },
    },
    body(files),
  );
  const at = files.rows.findIndex((r) => r.path === files.selected);
  if (at >= 0) tree.setAttribute('aria-activedescendant', `cf-r${at}`);
  return el(
    'section',
    { class: 'cfiles', id: 'cfiles', 'aria-label': `Files of ${files.name}` },
    head(files),
    tools(files),
    crumb(files),
    files.truncated &&
      el(
        'div',
        { class: 'cf-note warn' },
        'A folder is too large to list whole; the first part is shown.',
      ),
    tree,
    foot(files),
  );
}

/** What a redraw of the stage needs to hand back to the panel: its tree's
 *  scroll and where the keys were. Taken before the old panel goes. */
export function beforeRedraw(): { scroll: number; keys: boolean; filter: boolean; caret: number } {
  const tree = document.getElementById('cfiles-tree');
  const filter = document.activeElement?.id === 'cfiles-filter';
  const input = document.getElementById('cfiles-filter');
  return {
    scroll: tree?.scrollTop ?? 0,
    keys: filesFocused() || keepKeys,
    filter,
    caret: input instanceof HTMLInputElement ? (input.selectionStart ?? 0) : 0,
  };
}

export function afterRedraw(before: ReturnType<typeof beforeRedraw>): void {
  const files = filesOf();
  const tree = document.getElementById('cfiles-tree');
  if (!files || !tree) {
    drawnFor = null;
    return;
  }
  tree.scrollTop = before.scroll;
  tree.querySelector('.cf-row.on')?.scrollIntoView({ block: 'nearest' });
  const opened = drawnFor !== files.checkout;
  drawnFor = files.checkout;
  const input = document.getElementById('cfiles-filter');
  if (before.filter && input instanceof HTMLInputElement) {
    input.focus();
    input.setSelectionRange(before.caret, before.caret);
  } else if (before.keys || opened) {
    tree.focus({ focusVisible: false } as FocusOptions);
  }
  keepKeys = false;
}

/** The files button of a table row is lit while that checkout's are open. */
export function filesOpenFor(checkout: number | null): boolean {
  return checkout !== null && filesOf()?.checkout === checkout;
}
