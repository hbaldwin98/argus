// The comment box under picked lines, the review's and the Code stage's
// alike: what the lines are, the agent it goes to, and the words typed,
// kept here across redraws. Rust says whether it is open and which agent
// is lit (`src/review/comment_box.rs`); the page says what was typed,
// chosen or sent.

import type { CommentPage } from '../bindings/CommentPage';
import { el } from '../kit/dom.js';
import { actionFor } from '../keys/keymap.js';

export type CommentBoxId = 'review-comment' | 'code-comment';

/** How one surface's box is heard: its textarea's id, which that surface's
 *  focus rule looks for, and its two calls. */
export type CommentWays = {
  id: CommentBoxId;
  /** Sends the body, or with null puts the box away. */
  send: (body: string | null) => void;
  choose: (pane: number) => void;
};

const drafts: Record<CommentBoxId, string> = { 'review-comment': '', 'code-comment': '' };

/** Lets the words typed go once the box is shut, so the next starts empty. */
export function settleDraft(id: CommentBoxId, open: boolean): void {
  if (!open) drafts[id] = '';
}

export function commentBox(c: CommentPage, ways: CommentWays): HTMLElement {
  const send = (): void => ways.send(drafts[ways.id]);
  const body = el('textarea', {
    class: 'field comment-body',
    id: ways.id,
    'aria-label': 'Comment',
    rows: '3',
    oninput: (e: Event) => {
      if (e.target instanceof HTMLTextAreaElement) drafts[ways.id] = e.target.value;
    },
    onkeydown: (e: KeyboardEvent) => {
      const action = actionFor(['writing'], e);
      if (action === 'save') {
        e.preventDefault();
        send();
      } else if (action === 'closeLayer') {
        e.preventDefault();
        e.stopPropagation();
        ways.send(null);
      }
    },
  });
  body.value = drafts[ways.id];
  // With none lit (the agent chosen has gone) every one left is a button,
  // so there is someone to click.
  const lit = c.recipients.find((p) => p.pane === c.to);
  return el(
    'div',
    { class: 'comment-card' },
    el(
      'div',
      { class: 'actions' },
      el('span', { class: 'fg-accent strong' }, 'Comment'),
      el('span', { class: 'meta' }, c.about),
    ),
    body,
    el(
      'div',
      { class: 'actions' },
      el('span', { class: 'meta' }, 'Send to'),
      lit && c.recipients.length === 1
        ? el('span', { class: 'chipf on' }, el('span', { class: 'glyph' }, '◆'), lit.title)
        : c.recipients.map((p) =>
            el(
              'button',
              {
                class: `chipf${p.pane === c.to ? ' on' : ''}`,
                onclick: () => ways.choose(p.pane),
              },
              el('span', { class: 'glyph' }, '◆'),
              p.title,
            ),
          ),
      el('span', { class: 'grow' }),
      el('button', { class: 'btn', onclick: () => ways.send(null) }, 'Cancel'),
      el(
        'button',
        { class: 'btn primary', onclick: send },
        'Comment',
        el('span', { class: 'kbd on-accent' }, 'Ctrl Enter'),
      ),
    ),
  );
}
