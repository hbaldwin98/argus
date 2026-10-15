// The buttons an agent put in a view, and what a press of one sends: the
// agent is told through the daemon, as a refresh is asked of it, and the
// view changes only when the agent shows it again.
import { el } from './dom.js';
import { call } from './tauri.js';
/** A button for `action` in the view `ctx` is in: a press tells the agent. */
export function actionButton(action, ctx, with_ = () => ({})) {
    const press = () => {
        const extra = with_();
        const sent = {
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
