// What a session said, drawn: a line of it, as the recent-sessions
// preview shows it, and the full-transcript reader that opens in place of
// that dialog's list — the person's turns as a jump list, a find, Copy
// session id and Resume. Rust holds where the reader is and what it found
// (`src/agents/transcript_reader.rs`); the page draws it and says what
// was pressed.

import type { Action } from '../bindings/Action';
import type { ReaderPage } from '../bindings/ReaderPage';
import type { SessionPreview } from '../bindings/SessionPreview';
import type { TranscriptLine } from '../bindings/TranscriptLine';
import { clock, hhmm, span } from '../kit/clock.js';
import { el, ghosts, icon, type Child, type IconName } from '../kit/dom.js';
import { actionFor } from '../keys/keymap.js';
import { call } from '../connection/tauri.js';

function keycap(key: string): HTMLElement {
  return el('span', { class: 'kbd' }, key);
}

function act(action: Action): void {
  call('sessions_action', { action, row: null });
}

/**
 * A tool call told as prose with a small icon, the way a person would say
 * it: "Read", "Edited", "Ran", "Searched for". A tool this does not know
 * keeps its own name and no icon, since a wrong verb would misreport what
 * the agent did. Harnesses name the same act differently (`Bash`, `shell`).
 */
const VERBS: Record<string, [IconName, string]> = {
  read: ['tool-read', 'Read'],
  view: ['tool-read', 'Read'],
  grep: ['tool-search', 'Searched for'],
  glob: ['tool-search', 'Searched for'],
  search: ['tool-search', 'Searched for'],
  write: ['tool-edit', 'Wrote'],
  edit: ['tool-edit', 'Edited'],
  multiedit: ['tool-edit', 'Edited'],
  notebookedit: ['tool-edit', 'Edited'],
  apply_patch: ['tool-edit', 'Edited'],
  bash: ['tool-run', 'Ran'],
  shell: ['tool-run', 'Ran'],
  exec_command: ['tool-run', 'Ran'],
};

export function toolVerb(tool: string): [IconName, string] | null {
  return VERBS[tool.toLowerCase()] ?? null;
}

/** One thing said or done; `mark` is its find highlight, if any. */
export function said(l: TranscriptLine, id: string | null = null, mark = ''): HTMLElement {
  if (l.kind === 'tool') {
    const verb = toolVerb(l.tool);
    return el(
      'div',
      { class: `tline tool${l.failed ? ' failed' : ''}${mark}`, id },
      verb && icon(verb[0]),
      el('span', { class: 'ttool' }, verb ? verb[1] : l.tool),
      el('span', { class: 'mono ellipsis' }, l.text),
      l.failed && el('span', { class: 'fg-danger' }, 'failed'),
    );
  }
  const at = hhmm(l.at);
  return el(
    'div',
    { class: `tline ${l.kind}${mark}`, id },
    el('div', { class: 'twho meta' }, l.kind === 'user' ? 'You' : 'Agent', at && ` · ${at}`),
    el('div', { class: 'ttext' }, l.text),
  );
}

/** The session's lines, or why there are none. */
function body(r: ReaderPage): Child {
  const p = r.session;
  if (p.loading) return ghosts([[0.5], [0.9], [0.8], [0.4], [0.85], [0.6]]);
  if (p.notice) return el('div', { class: 'review-empty fg-muted' }, p.notice);
  const matched = new Set(r.matches);
  return [
    p.earlier > 0 &&
      el(
        'div',
        { class: 'meta tearlier' },
        `${p.earlier} earlier entr${p.earlier === 1 ? 'y' : 'ies'} not kept: the newest are read`,
      ),
    p.entries.map((l, i) =>
      said(l, `entry-${i}`, r.found === i ? ' found' : matched.has(i) ? ' matched' : ''),
    ),
  ];
}

/** The person's turns, to jump between. */
function jumps(r: ReaderPage): HTMLElement {
  return el(
    'aside',
    { class: 'reader-turns', id: 'reader-turns', 'aria-label': 'Your turns' },
    el('div', { class: 'label' }, 'Your turns'),
    r.jumps.length === 0 &&
      !r.session.loading &&
      el('div', { class: 'meta' }, 'Nothing was asked in what was kept.'),
    r.jumps.map((j, i) =>
      el(
        'button',
        {
          class: `frow${j.on ? ' on' : ''}`,
          title: j.text,
          onclick: () => call('sessions_action', { action: 'next', row: i }),
        },
        el('span', { class: 'ttl' }, j.text),
        el('span', { class: 'meta' }, hhmm(j.at) ?? ''),
      ),
    ),
    el(
      'div',
      { class: 'meta reader-note' },
      `Read from ${r.session.harness}'s own transcript file. A harness without one lists its sessions but cannot show them.`,
    ),
  );
}

/**
 * Rust finds; the field only says what it says. Its text is its own once
 * drawn, as the sessions search is.
 */
function findField(r: ReaderPage): HTMLElement {
  const live = document.getElementById('transcript-find');
  const field = el('input', {
    class: 'field filter',
    id: 'transcript-find',
    placeholder: 'Find in this transcript',
    'aria-label': 'Find in this transcript',
    oninput: (e: Event) => {
      if (e.target instanceof HTMLInputElement)
        call('find_in_transcript', { find: e.target.value });
    },
    onkeydown: (e: KeyboardEvent) => {
      // The reader has the keys back, on the match found.
      if (actionFor(['filter'], e) === 'leave') {
        e.preventDefault();
        (e.target as HTMLElement).blur();
      }
    },
  });
  field.value = live instanceof HTMLInputElement ? live.value : r.find;
  return field;
}

function count(r: ReaderPage): Child {
  if (r.find.trim() === '') return null;
  if (r.matches.length === 0) return el('span', { class: 'meta' }, 'no match');
  const at = r.found === null ? 0 : r.matches.indexOf(r.found) + 1;
  return el('span', { class: 'meta' }, `${at} of ${r.matches.length}`);
}

/** What the session offers, as the dialog's footer names it. */
function offer(p: SessionPreview): Child {
  if (p.offer === 'go') {
    return el(
      'button',
      { class: 'btn sm primary', onclick: () => act('choose') },
      'Go to its pane ',
      el('span', { class: 'kbd on-accent' }, 'Enter'),
    );
  }
  if (p.offer === 'resume') {
    return el(
      'button',
      { class: 'btn sm primary', title: p.command, onclick: () => act('resume') },
      'Resume in a new pane ',
      el('span', { class: 'kbd on-accent' }, 'R'),
    );
  }
  return p.why && el('span', { class: 'meta ellipsis' }, `Not resumable: ${p.why}`);
}

/** The reader, in the dialog in place of its list. */
export function drawReader(r: ReaderPage): HTMLElement {
  const p = r.session;
  const turns = r.turns === null ? null : `${r.turns} turn${r.turns === 1 ? '' : 's'}`;
  return el(
    'section',
    {
      class: 'dialog sheet-dialog review-dialog',
      role: 'dialog',
      'aria-modal': 'true',
      'aria-labelledby': 'reader-title',
    },
    el(
      'div',
      { class: 'dhead review-head' },
      el(
        'button',
        { class: 'btn sm', onclick: () => act('closeLayer') },
        '‹ Recent sessions ',
        keycap('Esc'),
      ),
      el(
        'div',
        { class: 'stack-2 reader-what' },
        el(
          'div',
          { class: 'reader-title' },
          el('span', { class: 'type-heading ellipsis', id: 'reader-title' }, p.title),
          el('span', { class: 'harn' }, p.harness),
        ),
        el(
          'div',
          { class: 'meta ellipsis' },
          [
            r.repository,
            el('span', { class: 'mono fg-ref' }, p.checkout),
            span(p.started, r.ended) ?? clock(r.ended),
            turns,
          ]
            .filter((part) => part)
            .flatMap((part, i) => (i === 0 ? [part] : [' · ', part])),
        ),
      ),
      el('span', { class: 'grow' }),
      findField(r),
      count(r),
      el(
        'button',
        { class: 'btn sm', title: p.sessionId, onclick: () => act('copyId') },
        'Copy session id ',
        keycap('Y'),
      ),
      offer(p),
    ),
    el(
      'div',
      { class: 'review-split' },
      jumps(r),
      el(
        'section',
        { class: 'review-diff' },
        el('div', { class: 'dbody transcript', id: 'reader-body' }, body(r)),
      ),
    ),
    el(
      'div',
      { class: 'dfoot meta' },
      keycap('J'),
      keycap('K'),
      ' your turns · ',
      keycap('/'),
      ' find · ',
      keycap('N'),
      ' next match · ',
      keycap('Y'),
      ' copy id · ',
      keycap('Esc'),
      ' back to the list',
      el('span', { class: 'grow' }),
      el('span', { class: 'mono ellipsis' }, p.sessionId),
    ),
  );
}

/** The session the reader last drew read, and the move it last scrolled to. */
let drawn: { session: string | null; moves: number } = { session: null, moves: 0 };

/**
 * After a redraw: a transcript newly read opens on how it ended, which is
 * what it is read before resuming for; a move scrolls to where it went;
 * anything else stays where it was scrolled.
 */
export function placeReader(r: ReaderPage | null, scrolled: number): void {
  const bodyNode = document.getElementById('reader-body');
  const read = r && !r.session.loading ? r.session.sessionId : null;
  if (!r || !bodyNode) {
    drawn = { session: null, moves: 0 };
    return;
  }
  bodyNode.scrollTop = scrolled;
  if (read !== drawn.session) bodyNode.scrollTop = bodyNode.scrollHeight;
  if (r.moves !== drawn.moves && r.target !== null) {
    document.getElementById(`entry-${r.target}`)?.scrollIntoView({ block: 'start' });
  }
  document.querySelector('#reader-turns .frow.on')?.scrollIntoView({ block: 'nearest' });
  drawn = { session: read, moves: r.moves };
}

/** `/`: the find field takes the keys. */
export function focusTranscriptFind(): void {
  document.getElementById('transcript-find')?.focus();
}
