// The review dialog: a checkout's changed files beside the diff of one,
// lines picked and commented on in place. Rust holds the diff, the cursor
// and the range (`src/review/dialog.rs`); the page draws them and says
// what was pressed or clicked. A redraw keeps the scroll, and the comment
// being written, where they were.

import type { DiffRow } from '../bindings/DiffRow';
import type { DiffSide } from '../bindings/DiffSide';
import type { ReviewClick } from '../bindings/ReviewClick';
import type { ReviewJumpGesture } from '../bindings/ReviewJumpGesture';
import type { ReviewPage } from '../bindings/ReviewPage';
import { commentBox, settleDraft, type CommentWays } from './comment_box.js';
import { $, el, ghosts, icon, type Child } from '../kit/dom.js';
import { actionFor } from '../keys/keymap.js';
import { state } from '../state.js';
import { call } from '../connection/tauri.js';

function keycap(key: string): HTMLElement {
  return el('span', { class: 'kbd' }, key);
}

function segment(label: string, on: boolean, pick: () => void, ...rest: Child[]): HTMLElement {
  return el(
    'button',
    { class: `segb${on ? ' on' : ''}`, role: 'radio', 'aria-checked': String(on), onclick: pick },
    label,
    ...rest,
  );
}

function count(n: number | null): HTMLElement {
  return el('span', { class: 'fg-faint' }, n === null ? '…' : String(n));
}

function head(r: ReviewPage): HTMLElement {
  return el(
    'div',
    { class: 'dhead review-head' },
    el('div', { class: 'type-title', id: 'review-title' }, 'Review'),
    // The branch once, as a chip: the checkout's name is its tooltip.
    el(
      'span',
      { class: 'chip-branch', title: r.name },
      icon('branch'),
      el('span', { class: 'mono' }, r.branch),
    ),
    r.base === 'commit'
      ? el('span', { class: 'meta mono' }, r.commit ?? '')
      : [
          el('span', { class: 'seg-label' }, 'Changes'),
          el(
            'div',
            { class: 'seg', role: 'radiogroup', 'aria-label': 'Side' },
            segment(
              'Unstaged ',
              r.base === 'unstaged',
              () => click({ on: 'side', staged: false }),
              count(r.unstaged),
            ),
            segment(
              'Staged ',
              r.base === 'staged',
              () => click({ on: 'side', staged: true }),
              count(r.staged),
            ),
          ),
        ],
    el('span', { class: 'seg-label' }, 'View'),
    el(
      'div',
      { class: 'seg', role: 'radiogroup', 'aria-label': 'Layout' },
      segment('Unified', !r.split, () => !r.split || call('review_action', { action: 'split' })),
      segment('Split', r.split, () => r.split || call('review_action', { action: 'split' })),
    ),
    r.files.length > 0 &&
      el(
        'span',
        { class: 'meta' },
        `${r.files.length} file${r.files.length === 1 ? '' : 's'} · `,
        el('span', { class: 'fg-ok' }, `+${r.added}`),
        ' ',
        el('span', { class: 'fg-danger' }, `−${r.removed}`),
      ),
    el('span', { class: 'grow' }),
    el(
      'button',
      {
        class: 'icon',
        'aria-label': 'Refresh',
        title: 'Refresh',
        onclick: () => call('review_refresh', {}),
      },
      icon('refresh'),
    ),
    el(
      'button',
      {
        class: 'icon',
        'aria-label': 'Close the review',
        onclick: () => call('review_action', { action: 'closeLayer' }),
      },
      icon('close'),
    ),
  );
}

const ways: CommentWays = {
  id: 'review-comment',
  send: (body) => call('review_comment', { body }),
  choose: (pane) => click({ on: 'recipient', pane }),
};

function click(target: ReviewClick): void {
  call('review_click', { target });
}

function jump(gesture: ReviewJumpGesture): void {
  call('review_jump', { gesture });
}

/**
 * The Ctrl-P field: its keys are its own, and Rust says what matches. The
 * text is the field's own once it is open: Rust's echo of a letter can
 * land after the next is typed, and putting it back would drop that one.
 */
function jumpField(query: string): HTMLElement {
  const live = document.getElementById('review-jump');
  const field = el('input', {
    class: 'field',
    id: 'review-jump',
    placeholder: 'Jump to a file',
    'aria-label': 'Jump to a file',
    oninput: (e: Event) => {
      if (e.target instanceof HTMLInputElement) jump({ on: 'query', text: e.target.value });
    },
    onkeydown: (e: KeyboardEvent) => {
      // Ctrl-N and Ctrl-P step as in fzf, and must not reach the webview,
      // where Ctrl-P prints.
      const action = actionFor(['jump'], e);
      if (action === 'next' || action === 'previous') {
        jump({ on: 'step', by: action === 'next' ? 1 : -1 });
      } else if (action === 'choose') jump({ on: 'choose' });
      else if (action === 'closeLayer') {
        // The field closes, not the dialog under it.
        e.stopPropagation();
        jump({ on: 'close' });
      } else return;
      e.preventDefault();
    },
  });
  field.value = live instanceof HTMLInputElement ? live.value : query;
  return field;
}

function files(r: ReviewPage): HTMLElement {
  const order = r.jump ? r.jump.matches : r.files.map((_, i) => i);
  const lit = r.jump ? r.jump.at : r.file;
  return el(
    'aside',
    { class: 'review-files', 'aria-label': 'Changed files' },
    r.jump ? jumpField(r.jump.query) : el('div', { class: 'mlabel' }, 'Changed files'),
    el(
      'div',
      { class: 'review-file-list' },
      order.map((i) => {
        const f = r.files[i];
        return (
          f &&
          el(
            'button',
            {
              class: `rfile${i === lit ? ' on' : ''}`,
              title: f.path,
              onclick: () => click({ on: 'file', file: i }),
            },
            // The filename first, on its own line: a path cut short loses it.
            el(
              'span',
              { class: 'rname' },
              el('span', { class: 'rbase' }, f.name),
              el('span', { class: 'rdir' }, f.dir || 'repository root'),
            ),
            el('span', { class: 'fg-ok' }, `+${f.added}`),
            el('span', { class: 'fg-danger' }, `−${f.removed}`),
          )
        );
      }),
      r.jump &&
        r.jump.matches.length === 0 &&
        el('div', { class: 'meta' }, `No file matches “${r.jump.query}”.`),
    ),
  );
}

function code(side: DiffSide): HTMLElement {
  return el(
    'span',
    { class: 'dcode' },
    side.parts.map(([text, kind]) => (kind ? el('span', { class: `sx-${kind}` }, text) : text)),
  );
}

function no(n: number | null | undefined): HTMLElement {
  return el('span', { class: 'dno' }, n == null ? '' : String(n));
}

function numbers(side: DiffSide | null, both: boolean): HTMLElement[] {
  return both
    ? [no(side?.old), no(side?.new)]
    : [no(side?.kind === 'added' ? side.new : (side?.old ?? side?.new))];
}

function half(side: DiffSide | null, both: boolean): HTMLElement {
  return el(
    'div',
    { class: `dside ${side?.kind ?? 'blank'}` },
    numbers(side, both),
    el('span', { class: 'dmark' }, side?.marker ?? ''),
    side ? code(side) : el('span', { class: 'dcode' }),
  );
}

/**
 * A row of a diff; `pick` is what a click on a line does, if anything, and
 * `unfold` what a fold's buttons do. A fold drawn without one is only said.
 */
export function diffRow(
  r: DiffRow,
  pick?: (e: MouseEvent) => void,
  unfold?: (all: boolean) => void,
): HTMLElement {
  if (r.kind === 'hunk' || r.kind === 'note') {
    return el('div', { class: `drow ${r.kind}` }, r.text);
  }
  if (r.kind === 'fold') {
    const show = (label: string, all: boolean): HTMLElement =>
      el(
        'button',
        {
          class: 'fbtn',
          onclick: (e: MouseEvent) => {
            // The row under it would take the click as a move first.
            e.stopPropagation();
            unfold?.(all);
          },
        },
        label,
      );
    return el(
      'div',
      { class: `drow fold${r.cursor ? ' cursor' : ''}`, onclick: pick ?? null },
      el('span', { class: 'fold-text' }, `⋯ ${r.text}`),
      unfold &&
        (r.step ? [show(`Show ${r.step}`, false), show('Show all', true)] : show('Show', true)),
    );
  }
  const line = r.kind === 'line';
  return el(
    'div',
    {
      class: `drow ${r.kind}${r.cursor ? ' cursor' : ''}${r.picked ? ' picked' : ''}`,
      onclick: pick ?? null,
    },
    line ? half(r.left, true) : [half(r.left, false), half(r.right, false)],
  );
}

function diff(r: ReviewPage): HTMLElement {
  const path = r.file === null ? null : r.files[r.file];
  let body: Child;
  if (r.loading) body = ghosts([[0.4], [0.9], [0.8], [0.85], [0.6], [0.75]]);
  else if (r.empty) body = el('div', { class: 'review-empty fg-muted' }, r.empty);
  else {
    // The comment box sits under the last picked line, or the cursor's.
    const rows = r.rows;
    let under = -1;
    rows.forEach((d, i) => {
      if (d.picked || (d.cursor && under < 0)) under = i;
    });
    body = rows.flatMap((d, i) => [
      diffRow(
        d,
        (e) => click({ on: 'row', row: d.row, extend: e.shiftKey }),
        (all) => click({ on: 'fold', row: d.row, all }),
      ),
      r.comment && i === under && commentBox(r.comment, ways),
    ]);
  }
  return el(
    'section',
    { class: 'review-diff' },
    path &&
      el(
        'div',
        { class: 'fhdr' },
        el('span', { class: 'mono' }, path.path),
        el('span', { class: 'grow' }),
        r.pick && el('span', { class: 'meta' }, `${r.pick} picked`),
      ),
    // Not `split`: that class is the app's flex-row layout, and would lay
    // every diff row side by side.
    el('div', { class: `dbody${r.split ? ' paired' : ''}`, id: 'review-body' }, body),
  );
}

export function drawReview(): void {
  const r = state.review;
  const host = $('review');
  if (!r?.open) {
    host.hidden = true;
    host.replaceChildren();
    settleDraft('review-comment', false);
    return;
  }
  // What a redraw would lose: where each list was scrolled, and whether
  // the comment had the keys.
  const scrolled = ['review-body', 'review-files'].map(
    (id) => document.getElementById(id)?.scrollTop ?? 0,
  );
  const typing = document.activeElement?.id === 'review-comment';
  settleDraft('review-comment', r.comment !== null);
  host.hidden = false;
  const fileList = files(r);
  fileList.id = 'review-files';
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
          'aria-labelledby': 'review-title',
        },
        head(r),
        el('div', { class: 'review-split' }, fileList, diff(r)),
        el(
          'div',
          { class: 'dfoot meta' },
          keycap('J'),
          keycap('K'),
          ' line · ',
          keycap('V'),
          ' pick · ',
          keycap('C'),
          ' comment · ',
          keycap('B'),
          ' side · ',
          keycap('S'),
          ' split · ',
          keycap('Ctrl P'),
          ' jump · ',
          keycap('E'),
          ' editor · ',
          keycap('Enter'),
          ' unfold · ',
          keycap('Esc'),
          ' close',
        ),
      ),
    ),
  );
  ['review-body', 'review-files'].forEach((id, i) => {
    const node = document.getElementById(id);
    if (node) node.scrollTop = scrolled[i] ?? 0;
  });
  host.querySelector('.drow.cursor')?.scrollIntoView({ block: 'nearest' });
  host.querySelector('.rfile.on')?.scrollIntoView({ block: 'nearest' });
  // The jump field is redrawn as each letter arrives, and keeps the keys
  // and the caret while it is open.
  const field = document.getElementById('review-jump');
  const box = document.getElementById('review-comment');
  const keys =
    field instanceof HTMLInputElement
      ? field
      : box instanceof HTMLTextAreaElement && (typing || r.comment !== null)
        ? box
        : null;
  if (keys) {
    keys.focus();
    keys.setSelectionRange(keys.value.length, keys.value.length);
  }
}

export function reviewOpen(): boolean {
  return state.review?.open === true;
}
