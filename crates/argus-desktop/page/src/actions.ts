// The buttons an agent put in a view, and what a press of one sends: the
// agent is told through the daemon, as a refresh is asked of it, and the
// view changes only when the agent shows it again.

import type { ViewAction } from './bindings/ViewAction';
import type { Answer } from './bindings/Answer';
import type { ViewPress } from './bindings/ViewPress';
import { el } from './dom.js';
import { call } from './tauri.js';
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

/** A button for `action` in the view `ctx` is in: a press tells the agent. */
export function actionButton(
  action: ViewAction,
  ctx: ViewCtx,
  with_: () => PressWith = () => ({}),
): HTMLElement {
  const press = (): void => {
    const extra = with_();
    const sent: Press = {
      view: ctx.id,
      action: action.id,
      rows: [...(extra.rows ?? [])],
      form: extra.form ?? null,
      answers: [...(extra.answers ?? [])],
    };
    call('press_view', { pane: ctx.pane, press: sent });
  };
  return el('button', { class: 'btn sm', type: 'button', onclick: press }, action.label);
}
