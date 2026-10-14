// The review dialog: a checkout's changed files beside the diff of one,
// lines picked and commented on in place. Rust holds the diff, the cursor
// and the range (`src/review.rs`); the page draws them and says what was
// pressed or clicked. A redraw keeps the scroll, and the comment being
// written, where they were.

import type { DiffRow } from './bindings/DiffRow';
import type { DiffSide } from './bindings/DiffSide';
import type { ReviewPage } from './bindings/ReviewPage';
import { $, el, ghosts, icon, type Child } from './dom.js';
import { state } from './state.js';
import { call } from './tauri.js';

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
    el(
      'div',
      { class: 'type-title', id: 'review-title' },
      'Review · ',
      r.name,
      ' ',
      el('span', { class: 'mono fg-ref' }, r.branch),
    ),
    r.base === 'commit'
      ? el('span', { class: 'meta mono' }, r.commit ?? '')
      : el(
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
    el('button', { class: 'btn sm', onclick: () => call('review_refresh', {}) }, 'Refresh'),
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

type Click =
  | { on: 'file'; file: number }
  | { on: 'row'; row: number; extend: boolean }
  | { on: 'side'; staged: boolean }
  | { on: 'recipient'; pane: number };

function click(target: Click): void {
  call('review_click', { target });
}

function files(r: ReviewPage): HTMLElement {
  return el(
    'aside',
    { class: 'review-files', 'aria-label': 'Changed files' },
    el('div', { class: 'mlabel' }, 'Changed files'),
    el(
      'div',
      { class: 'review-file-list' },
      r.files.map((f, i) =>
        el(
          'button',
          {
            class: `rfile${i === r.file ? ' on' : ''}`,
            title: f.path,
            onclick: () => click({ on: 'file', file: i }),
          },
          el('span', { class: 'rname' }, el('span', { class: 'fg-faint' }, f.dir), f.name),
          el('span', { class: 'fg-ok' }, `+${f.added}`),
          el('span', { class: 'fg-danger' }, `−${f.removed}`),
        ),
      ),
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

/** A row of a diff; `pick` is what a click on a line does, if anything. */
export function diffRow(r: DiffRow, pick?: (e: MouseEvent) => void): HTMLElement {
  if (r.kind === 'hunk' || r.kind === 'note') {
    return el('div', { class: `drow ${r.kind}` }, r.text);
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

function commentBox(r: ReviewPage): HTMLElement {
  const body = el('textarea', {
    class: 'field comment-body',
    id: 'review-comment',
    'aria-label': 'Comment',
    rows: '3',
    oninput: (e: Event) => {
      if (e.target instanceof HTMLTextAreaElement) state.reviewDraft = e.target.value;
    },
    onkeydown: (e: KeyboardEvent) => {
      if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
        e.preventDefault();
        send();
      } else if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        call('review_comment', { body: null });
      }
    },
  });
  body.value = state.reviewDraft;
  const chosen = r.recipients.find((p) => p.pane === r.recipient);
  return el(
    'div',
    { class: 'comment-card' },
    el(
      'div',
      { class: 'actions' },
      el('span', { class: 'fg-accent strong' }, 'Comment'),
      el('span', { class: 'meta' }, `${r.pick ?? ''} · ${r.base}`),
    ),
    body,
    el(
      'div',
      { class: 'actions' },
      el('span', { class: 'meta' }, 'Send to'),
      r.recipients.length > 1
        ? r.recipients.map((p) =>
            el(
              'button',
              {
                class: `chipf${p.pane === r.recipient ? ' on' : ''}`,
                onclick: () => click({ on: 'recipient', pane: p.pane }),
              },
              el('span', { class: 'glyph' }, '◆'),
              p.title,
            ),
          )
        : el(
            'span',
            { class: 'chipf on' },
            el('span', { class: 'glyph' }, '◆'),
            chosen?.title ?? '',
          ),
      el('span', { class: 'grow' }),
      el(
        'button',
        { class: 'btn', onclick: () => call('review_comment', { body: null }) },
        'Cancel',
      ),
      el('button', { class: 'btn primary', onclick: send }, 'Comment ', keycap('Ctrl Enter')),
    ),
  );
}

function send(): void {
  call('review_comment', { body: state.reviewDraft });
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
      diffRow(d, (e) => click({ on: 'row', row: d.row, extend: e.shiftKey })),
      r.composing && i === under && commentBox(r),
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
    el('div', { class: `dbody${r.split ? ' split' : ''}`, id: 'review-body' }, body),
  );
}

export function drawReview(): void {
  const r = state.review;
  const host = $('review');
  if (!r?.open) {
    host.hidden = true;
    host.replaceChildren();
    state.reviewDraft = '';
    return;
  }
  // What a redraw would lose: where each list was scrolled, and whether
  // the comment had the keys.
  const scrolled = ['review-body', 'review-files'].map(
    (id) => document.getElementById(id)?.scrollTop ?? 0,
  );
  const typing = document.activeElement?.id === 'review-comment';
  if (!r.composing) state.reviewDraft = '';
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
          class: 'dialog review-dialog',
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
  const box = document.getElementById('review-comment');
  if (box instanceof HTMLTextAreaElement && (typing || r.composing)) {
    box.focus();
    box.setSelectionRange(box.value.length, box.value.length);
  }
}

export function reviewOpen(): boolean {
  return state.review?.open === true;
}
