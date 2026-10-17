// The buttons an agent put in a view, and what a press of one sends: the
// agent is told through the daemon, as a refresh is asked of it, and the
// view changes only when the agent shows it again.

import type { ViewAction } from './bindings/ViewAction';
import type { Answer } from './bindings/Answer';
import type { ViewPress } from './bindings/ViewPress';
import { el } from './kit/dom.js';
import { call } from './connection/tauri.js';
import type { ViewCtx } from './viewctx.js';

/** What a press carries besides the action: the selected rows' keys, or a
 *  form's id and answers. Read at the moment of the press. */
export interface PressWith {
  rows?: readonly string[];
  form?: string;
  answers?: readonly Answer[];
}

/** What the page sends Rust for a press; the agent hears it as it is. */
export type Press = ViewPress;

/** The confirmations that are open, by button. A view is redrawn whole when
 *  anything in it changes, and a question the person is answering must not
 *  vanish under them. */
const asking = new Set<string>();

/** A button for `action` in the view `ctx` is in: a press tells the agent,
 *  after asking when the action has a `confirm`. While its press is
 *  unanswered the button waits, disabled, with a spinner. */
export function actionButton(
  action: ViewAction,
  ctx: ViewCtx,
  with_: () => PressWith = () => ({}),
): HTMLElement {
  const key = `${ctx.path}|${action.id}`;
  const slot = el('span', { class: 'vact' });
  const draw = (): void => slot.replaceChildren(asking.has(key) ? question() : button());

  const send = (): void => {
    const extra = with_();
    const sent: Press = {
      view: ctx.id,
      action: action.id,
      rows: [...(extra.rows ?? [])],
      form: extra.form ?? null,
      answers: [...(extra.answers ?? [])],
    };
    // A saved view has no pane: the daemon relays the press to whichever
    // agent last saved it.
    if (ctx.kind === 'saved') call('press_saved_view', { id: ctx.id, press: sent });
    else call('press_view', { pane: ctx.pane, press: sent });
  };

  const ask = (open: boolean): void => {
    if (open) asking.add(key);
    else asking.delete(key);
    draw();
  };

  const button = (): HTMLElement => {
    const waiting = ctx.pressed?.includes(action.id) ?? false;
    return el(
      'button',
      {
        class: `btn sm vtone-${action.tone}`,
        type: 'button',
        disabled: waiting || !ctx.canPress,
        title: ctx.canPress ? null : 'This Argus daemon cannot take button presses',
        onclick: () => (action.confirm ? ask(true) : send()),
      },
      waiting && el('span', { class: 'vspin', 'aria-label': 'waiting for the agent' }),
      action.label,
    );
  };

  const question = (): HTMLElement =>
    el(
      'span',
      { class: 'vconfirm', role: 'group', 'aria-label': action.confirm ?? '' },
      el('span', { class: 'vconfirm-q' }, action.confirm),
      el(
        'button',
        {
          class: `btn sm vtone-${action.tone}`,
          type: 'button',
          onclick: () => {
            ask(false);
            send();
          },
        },
        'Yes',
      ),
      el('button', { class: 'btn sm', type: 'button', onclick: () => ask(false) }, 'No'),
    );

  draw();
  return slot;
}
