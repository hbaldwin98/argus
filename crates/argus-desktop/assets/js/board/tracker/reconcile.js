// The reconcile dialog: what the person asked of an agent, its proposal
// grouped Into Argus and Out to each tracker with every line unticked, the
// line under the cursor in full — its exact text, editable before ticking,
// and the call the agent will make — and what it noticed and never acts
// on. With nothing under way it asks an agent. Rust holds the ticks and
// the words (`src/board/tracker/reconcile_dialog.rs`); the page draws
// them.
import { $, el, escKey, icon } from '../../kit/dom.js';
import { actionFor } from '../../keys/keymap.js';
import { call } from '../../connection/tauri.js';
let page = null;
function send(gesture) {
    call('reconcile', { gesture });
}
/** A key of the dialog's own, outside its fields. `edit` puts the keys in
 *  the selected line's text. */
export function reconcileAction(action) {
    if (action === 'edit') {
        const area = document.getElementById('reconcile-text');
        if (area instanceof HTMLTextAreaElement) {
            area.focus();
            area.setSelectionRange(area.value.length, area.value.length);
        }
        return;
    }
    send({ on: 'action', action });
}
function keycap(key) {
    return el('span', { class: 'kbd' }, key);
}
/** Escape in a field gives the dialog its keys back. */
function leaveOnEscape(e) {
    if (actionFor(['writing'], e) !== 'closeLayer')
        return false;
    e.preventDefault();
    e.target.blur();
    return true;
}
function lines(p) {
    const proposing = p.stage === 'proposed';
    return el('section', { class: 'rc-lines', 'aria-label': 'Proposed lines' }, p.groups.map((group) => el('div', { class: 'stack-6' }, el('div', { class: 'label' }, el('span', { class: group.intoArgus ? 'fg-ref' : 'fg-attention' }, group.heading), el('span', { class: 'grow' }), el('span', null, String(group.lines.length))), group.lines.map((row) => el('div', {
        class: `hrow rc-line${row.on ? ' on' : ''}`,
        onclick: () => send({ on: 'select', line: row.line }),
    }, 
    // The input is itself 24px (its box is drawn smaller), so the tick is easy to hit.
    el('input', {
        type: 'checkbox',
        class: 'rc-tick',
        'aria-label': `Tick ${row.key} ${row.does}`,
        checked: row.ticked,
        disabled: !proposing,
        onclick: (e) => {
            e.stopPropagation();
            send({ on: 'tick', line: row.line });
        },
    }), el('span', { class: 'mono fg-ref rc-key' }, row.key), el('span', { class: row.on ? 'strong' : '' }, row.does), el('span', { class: 'meta grow ellipsis' }, row.why), row.failed && el('span', { class: 'pill fg-danger', title: row.failed }, 'failed'))))), p.noticed.length > 0 &&
        el('div', { class: 'stack-6 rc-noticed' }, el('div', { class: 'label' }, 'Noticed, not proposed', el('span', { class: 'grow' }), 'outside what you asked'), p.noticed.map((n) => el('div', { class: 'hrow' }, el('span', { class: 'fg-faint' }, '·'), el('span', { class: 'mono fg-ref rc-key' }, n.label), el('span', { class: 'meta' }, n.text))), el('div', { class: 'meta' }, 'Tell the agent to include any of these and it adds a line. It never acts on them by itself.')));
}
function selected(p, line) {
    const editable = p.stage === 'proposed';
    const area = el('textarea', {
        class: 'field area rc-text mono',
        id: 'reconcile-text',
        'aria-label': 'The text, word for word',
        readonly: !editable,
        oninput: (e) => send({ on: 'text', line: line.line, text: e.target.value }),
        onkeydown: (e) => leaveOnEscape(e),
    });
    const live = document.getElementById('reconcile-text');
    area.value =
        live instanceof HTMLTextAreaElement && live.dataset.line === String(line.line)
            ? live.value
            : line.text;
    area.dataset.line = String(line.line);
    return el('section', { class: 'rc-detail stack-12' }, el('div', { class: 'stack-6' }, el('div', { class: 'label' }, line.heading, el('span', { class: 'grow' }), line.ticked ? 'ticked' : 'unticked'), el('div', { class: 'type-title' }, `${line.does} · ${line.key}`), line.failed && el('div', { class: 'meta fg-danger' }, `Failed last time: ${line.failed}`)), (line.text !== '' || editable) &&
        el('div', { class: 'stack-6' }, el('div', { class: 'label' }, 'The text, word for word', line.edited && el('span', { class: 'pill' }, 'edited')), area, editable && el('div', { class: 'meta' }, keycap('E'), ' edits it here before you tick it.')), line.call !== '' &&
        el('div', { class: 'stack-6' }, el('div', { class: 'label' }, 'What the agent will do'), el('pre', { class: 'rc-call mono' }, line.call)), el('div', { class: 'stack-6' }, el('div', { class: 'label' }, 'Afterwards'), el('div', { class: 'meta' }, 'The agent reports each ticked line as done or failed. A failed line comes back unticked, with the tracker’s error.')));
}
function ask(p) {
    const field = el('textarea', {
        class: 'field area',
        id: 'reconcile-ask',
        'aria-label': 'What to reconcile',
        placeholder: 'Pull ENG-88’s new sub-issues, mark 2.1 done in Linear, put D2 on the Jira ticket…',
        onkeydown: (e) => {
            if (leaveOnEscape(e))
                return;
            if (actionFor(['writing'], e) === 'save') {
                e.preventDefault();
                send({ on: 'ask', ask: field.value });
            }
        },
    });
    const live = document.getElementById('reconcile-ask');
    if (live instanceof HTMLTextAreaElement)
        field.value = live.value;
    return el('section', { class: 'stack-12 rc-ask' }, el('div', { class: 'label' }, 'You say what; it proposes; you tick'), field, p.agents.length === 0
        ? el('div', { class: 'meta fg-attention' }, 'No agent runs in this repository. Start one, then ask it here.')
        : el('div', { class: 'stack-6', role: 'radiogroup', 'aria-label': 'The agent to ask' }, p.agents.map((agent) => el('label', { class: 'hrow' }, el('input', {
            type: 'radio',
            name: 'reconcile-agent',
            checked: agent.chosen,
            onchange: () => send({ on: 'agent', pane: agent.pane }),
        }), el('span', null, agent.title), agent.last && el('span', { class: 'meta' }, 'did the last one')))), el('div', { class: 'actions' }, el('span', { class: 'meta grow' }, 'It reads only, then waits for you: nothing is written until you tick it.'), el('button', {
        class: 'btn sm primary',
        disabled: p.agents.length === 0,
        onclick: () => send({ on: 'ask', ask: field.value }),
    }, 'Send the ask ', el('span', { class: 'kbd on-accent' }, 'Ctrl+Enter'))));
}
function body(p) {
    if (p.stage === 'none')
        return ask(p);
    if (p.stage === 'asked') {
        return [
            el('div', { class: 'meta rc-waiting' }, `${p.by} is reading the map, the feature and the tickets; its proposal shows here when it sends one.`),
            ask(p),
        ];
    }
    return el('div', { class: 'rc-split' }, lines(p), p.selected
        ? selected(p, p.selected)
        : el('section', { class: 'rc-detail meta' }, 'The agent noticed things, and proposes nothing.'));
}
export function drawReconcile(next) {
    page = next;
    const host = $('reconcile');
    if (!next.open) {
        host.hidden = true;
        host.replaceChildren();
        return;
    }
    const focused = document.activeElement?.id ?? '';
    const caret = document.activeElement instanceof HTMLTextAreaElement
        ? [document.activeElement.selectionStart, document.activeElement.selectionEnd]
        : null;
    const p = next;
    host.hidden = false;
    const carrying = p.stage === 'carrying';
    host.replaceChildren(el('div', { class: 'scrim' }, el('section', {
        class: 'dialog sheet-dialog tracker-dialog',
        role: 'dialog',
        'aria-modal': 'true',
        'aria-labelledby': 'reconcile-title',
    }, el('div', { class: 'dhead' }, el('div', { class: 'stack-6 grow' }, el('div', { class: 'type-title', id: 'reconcile-title' }, p.tickets ? `Reconcile #${p.number} with ${p.tickets}` : `Reconcile #${p.number}`), p.asked &&
        el('div', { class: 'rc-asked' }, el('span', { class: 'label' }, 'You asked '), `“${p.asked}”`), el('div', { class: 'meta' }, p.by &&
        `${p.stage === 'asked' ? 'asked of' : 'proposed by'} ${p.by} · ${p.ago} ago · `, p.reconciled ?? 'never reconciled before'), p.stage === 'proposed' &&
        el('div', { class: 'meta' }, 'The agent has only read. Every line starts unticked: it writes nothing, in Argus or in a tracker, that you have not ticked.'), carrying &&
        el('div', { class: 'meta fg-attention' }, `${p.by} is carrying out the ${p.ticked} you ticked; nothing more is ticked until it reports.`)), el('button', { class: 'icon', 'aria-label': 'Close', onclick: () => send({ on: 'close' }) }, icon('close'))), el('div', { class: 'tracker-body' }, body(p)), el('div', { class: 'dfoot' }, el('span', {
        class: 'dwhy grow',
        title: 'J and K move, Space ticks, E edits the text, G goes to the agent.',
    }, p.stage === 'proposed' ? `${p.ticked} of ${p.total} ticked` : null), p.stage !== 'none' &&
        el('button', { class: 'btn', onclick: () => send({ on: 'discard' }) }, p.stage === 'asked' ? 'Take the ask back ' : 'Discard proposal ', keycap('D')), el('button', { class: 'btn', onclick: () => send({ on: 'close' }) }, 'Later ', escKey()), p.stage === 'proposed' &&
        el('button', {
            class: 'btn primary',
            disabled: p.ticked === 0,
            onclick: () => send({ on: 'carryOut' }),
        }, `Carry out the ${p.ticked} ticked `, el('span', { class: 'kbd on-accent' }, 'Enter'))))));
    // Redrawn as each letter arrives: the field typed in keeps the keys and
    // the caret.
    const again = focused.startsWith('reconcile-') ? document.getElementById(focused) : null;
    if (again instanceof HTMLTextAreaElement) {
        again.focus();
        if (caret)
            again.setSelectionRange(caret[0] ?? null, caret[1] ?? null);
    }
    host.querySelector('.rc-line.on')?.scrollIntoView({ block: 'nearest' });
}
export function reconcileOpen() {
    return page?.open === true;
}
