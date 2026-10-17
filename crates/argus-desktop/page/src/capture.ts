// A new feature written as one note: the note on the left, and on the right
// what it reads as. The reading is Rust's, by the daemon's own grammar, sent
// back on every change; the page never parses the note itself, so the
// preview is what Create stores. The note stays up after Create until the
// pushed board holds the feature, which then opens.

import type { CapturePage } from './bindings/CapturePage';
import { $, el, type Child } from './kit/dom.js';
import { actionFor } from './keymap.js';
import { state } from './state.js';
import { call } from './connection/tauri.js';

/** The grammar, as the foot of the note says it. */
const LEGEND: [string, string, string][] = [
  ['first line', '', 'the title'],
  ['a paragraph', '', 'the brief'],
  ['-', 'fg-accent', 'a task; indent it for a subtask'],
  ['?', 'fg-attention', 'an open decision; indented lines are its options'],
  ['!', 'fg-ok', 'a decision already made; the indented line is why'],
];

export function captureOpen(): boolean {
  return state.capture?.open === true;
}

export function openCapture(): void {
  call('capture', { gesture: { on: 'open' } });
}

export function discardCapture(): void {
  call('capture', { gesture: { on: 'discard' } });
}

function note(): HTMLTextAreaElement | null {
  const node = document.getElementById('capture-note');
  return node instanceof HTMLTextAreaElement ? node : null;
}

function create(): void {
  const text = note()?.value ?? state.capture?.note ?? '';
  call('capture', { gesture: { on: 'create', note: text } });
}

/** Draws the note, keeping the text field when it is already up: the
 *  reading comes back on every key, and redrawing the field would lose the
 *  caret. */
export function drawCapture(): void {
  const page = state.capture;
  if (!page?.open) return;
  const stage = $('fstage');
  const field = note();
  if (field && stage.contains(field)) {
    $('capture-head').replaceWith(head(page));
    $('capture-gutter').replaceWith(gutter(page, field.value));
    $('capture-reads').replaceWith(readsAs(page));
    return;
  }
  const textarea = el('textarea', {
    class: 'note-field',
    id: 'capture-note',
    'aria-label': 'Feature note',
    spellcheck: 'false',
    placeholder:
      'The title on the first line\n\nThe brief under it.\n\n- a task\n  - a subtask\n? an open question\n  - an option',
    oninput: () => {
      call('capture', { gesture: { on: 'note', note: textarea.value } });
      $('capture-gutter').replaceWith(gutter(state.capture ?? page, textarea.value));
    },
    onscroll: () => {
      const numbers = document.getElementById('capture-gutter');
      if (numbers) numbers.scrollTop = textarea.scrollTop;
    },
    onkeydown: (e: KeyboardEvent) => {
      const action = actionFor(['writing'], e);
      if (action === 'save') {
        e.preventDefault();
        create();
      } else if (action === 'closeLayer') {
        e.preventDefault();
        discardCapture();
      }
    },
  });
  textarea.value = page.note;
  stage.replaceChildren(
    head(page),
    el(
      'div',
      { class: 'split capture' },
      el(
        'section',
        { class: 'capture-note stack-8', 'aria-label': 'The note' },
        el('div', { class: 'label' }, 'Note'),
        el('div', { class: 'note-box' }, gutter(page, page.note), textarea),
        el(
          'div',
          { class: 'legend meta' },
          LEGEND.map(([mark, tone, means]) => [
            el('span', { class: tone ? `mono ${tone}` : '' }, mark),
            el('span', null, means),
          ]),
        ),
      ),
      readsAs(page),
    ),
  );
  textarea.focus();
}

function head(page: CapturePage): HTMLElement {
  return el(
    'div',
    { class: 'fhead capture-head', id: 'capture-head' },
    el(
      'div',
      { class: 'crumbs' },
      el('button', { class: 'crumb', onclick: discardCapture }, 'Features'),
      el('span', { class: 'fg-faint' }, '›'),
      el('span', { class: 'fg' }, 'New'),
      el('span', { class: 'grow' }),
      el('span', { class: 'meta' }, el('span', { class: 'kbd' }, 'Esc'), ' discard'),
    ),
    el(
      'div',
      { class: 'actions head-row' },
      el('span', { class: 'type-title' }, `New feature in ${page.repository}`),
      el(
        'span',
        { class: 'meta' },
        'Write it as one note. It becomes the feature when you create it.',
      ),
      el('span', { class: 'grow' }),
      el('button', { class: 'btn', onclick: discardCapture }, 'Discard'),
      el(
        'button',
        {
          class: 'btn primary',
          disabled: page.blocked !== null || page.creating,
          title: page.blocked ?? null,
          onclick: create,
        },
        page.creating ? 'Creating…' : 'Create',
        el('span', { class: 'kbd on-accent' }, 'Ctrl Enter'),
      ),
    ),
  );
}

/** Line numbers beside the note, a line the grammar could not place
 *  marked where it is. */
function gutter(page: CapturePage, text: string): HTMLElement {
  const lines = Math.max(1, text.split('\n').length);
  const numbers = el(
    'div',
    { class: 'note-gutter', id: 'capture-gutter', 'aria-hidden': 'true' },
    Array.from({ length: lines }, (_, i) =>
      el('div', { class: page.unread.includes(i + 1) ? 'fg-danger strong' : '' }, String(i + 1)),
    ),
  );
  const field = note();
  if (field) numbers.scrollTop = field.scrollTop;
  return numbers;
}

function readsAs(page: CapturePage): HTMLElement {
  let verdict: HTMLElement;
  if (page.creating) verdict = el('span', { class: 'label-aside' }, 'creating…');
  else if (page.blocked) verdict = el('span', { class: 'label-aside fg-attention' }, page.blocked);
  else verdict = el('span', { class: 'label-aside fg-ok' }, 'ready to create');
  const open = page.decisions.filter((d) => d.open).length;
  const decided = page.decisions.length - open;
  const unread: Child = page.unread.length > 0 && [
    el(
      'div',
      { class: 'fg-danger' },
      page.unread.length === 1
        ? `Line ${page.unread[0]} reads as nothing.`
        : `Lines ${page.unread.join(', ')} read as nothing.`,
      ' Start a task with -, a question with ?, a decision made with !.',
    ),
  ];
  return el(
    'section',
    {
      class: 'capture-reads stack-12',
      id: 'capture-reads',
      'aria-label': 'What the note reads as',
    },
    el(
      'div',
      { class: 'label' },
      el('span', null, 'Reads as'),
      el('span', { class: 'grow' }),
      verdict,
    ),
    el(
      'div',
      { class: 'cell stack-12' },
      unread,
      el(
        'div',
        { class: 'stack-4' },
        page.title
          ? el('div', { class: 'type-heading' }, page.title)
          : el('div', { class: 'fg-faint' }, 'The first line is the title.'),
        page.brief && el('div', { class: 'fg-muted prose' }, page.brief),
      ),
      el(
        'div',
        { class: 'stack-2' },
        el(
          'div',
          { class: 'label' },
          el('span', null, 'Tasks'),
          el('span', { class: 'grow' }),
          el('span', { class: 'label-aside' }, `0/${page.tasks.length}`),
        ),
        page.tasks.length === 0 &&
          el('div', { class: 'meta' }, 'A line starting with - is a task.'),
        page.tasks.map((task) =>
          el(
            'div',
            { class: 'nrow', style: `padding-left: ${task.depth * 20}px` },
            el('span', { class: 'fg-faint' }, '○'),
            el('span', { class: 'n' }, task.label),
            el('span', { class: task.depth ? 'fg-muted' : '' }, task.text),
          ),
        ),
      ),
      page.decisions.length > 0 &&
        el(
          'div',
          { class: 'stack-2' },
          el(
            'div',
            { class: 'label' },
            el('span', null, 'Decisions'),
            el('span', { class: 'grow' }),
            open > 0
              ? el('span', { class: 'label-aside fg-attention' }, `${open} open`)
              : el('span', { class: 'label-aside fg-ok' }, `${decided} decided`),
          ),
          page.decisions.map((decision) => [
            el(
              'div',
              { class: 'nrow' },
              el(
                'span',
                { class: decision.open ? 'fg-attention' : 'fg-ok' },
                decision.open ? '?' : '✓',
              ),
              el(
                'span',
                { class: `n ${decision.open ? 'fg-attention' : 'fg-ref'}` },
                decision.label,
              ),
              el('span', null, decision.text),
              el(
                'span',
                { class: `pill ${decision.open ? 'fg-attention' : 'fg-ok'}` },
                decision.open ? 'open' : 'made',
              ),
            ),
            decision.options.length > 0 &&
              el(
                'div',
                { class: 'meta indent' },
                decision.options.map((option, i) => el('div', null, `${i + 1} · ${option}`)),
              ),
            decision.because && el('div', { class: 'meta indent' }, `because ${decision.because}`),
          ]),
        ),
    ),
    el(
      'div',
      { class: 'meta' },
      "The daemon reads the note with the same grammar, so what is here is what it stores. A line it can't place is named here instead of dropped.",
    ),
  );
}
