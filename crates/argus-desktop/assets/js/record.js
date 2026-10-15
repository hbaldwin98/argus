// The Record lens of an open feature: every decision in the order it was
// made, as the tree it was made in, beside the one under the cursor read as
// its record. What a decision says, and the forms that change it, are
// `work.ts`'s, since the Work lens reads a decision the same way.
import { el } from './dom.js';
import { state } from './state.js';
import { call } from './tauri.js';
import { begin, DECISION_LINE, decisionAction, decisionDetail, editDecision, field, formButtons, stateMark, } from './work.js';
export function recordLens(view) {
    const record = view.record;
    const section = el('section', { class: 'work-outline', 'aria-label': 'Decisions' }, el('div', { class: 'label' }, el('span', null, 'Decisions'), el('span', { class: 'grow' }), el('span', null, 'in the order they were made')));
    const rows = record?.rows ?? [];
    if (!rows.length) {
        section.append(el('div', { class: 'meta' }, 'Nothing decided yet: raise a decision, made already or still open.'));
    }
    for (const row of rows)
        section.append(recordRow(row));
    const editing = state.editing;
    if (editing?.what === 'addDecision' && editing.task === null) {
        const what = editing.under === null ? 'Decision on the feature' : 'Sub-decision';
        section.append(el('div', { class: 'stack-4 work-add' }, el('div', { class: 'label' }, what), field('edit-add', editing.text, false, (text) => (editing.text = text), DECISION_LINE), formButtons('Add')));
    }
    if (rows.length) {
        section.append(el('div', { class: 'meta record-foot' }, 'Superseded and rejected decisions stay listed, struck through.'));
    }
    section.append(el('div', { class: 'actions work-foot' }, el('span', { class: 'grow' }), el('button', {
        class: 'btn',
        onclick: () => begin({ what: 'addDecision', under: null, task: null, text: '' }),
    }, '+ Decision')));
    const decision = record?.decision;
    return el('div', { class: 'split' }, section, decision
        ? decisionDetail(decision)
        : el('article', { class: 'work-detail' }, el('div', { class: 'meta' }, 'Choose a decision to read it as a record.')));
}
function recordRow(row) {
    const [mark, tone] = stateMark(row.state);
    const aside = row.state === 'superseded' || row.state === 'rejected' || row.state === 'withdrawn';
    const node = el('button', {
        class: `rrow${row.on ? ' on' : ''}`,
        'aria-current': row.on ? 'true' : null,
        onclick: () => call('select_decision', { id: row.id }),
        ondblclick: () => {
            const decision = state.features?.open?.record?.decision;
            if (decision)
                editDecision(decision);
        },
    }, el('span', { class: tone }, mark), el('span', { class: `n ${row.state === 'open' ? 'fg-attention' : aside ? 'fg-faint' : 'fg-ref'}` }, row.label), el('span', { class: `${row.on && !aside ? 'strong' : ''} ${aside ? 'fg-faint struck' : ''}` }, row.text), el('span', { class: `meta ${row.state === 'open' ? 'fg-attention' : ''}` }, row.state === 'accepted' ? '' : row.state));
    node.style.paddingLeft = `${8 + row.depth * 20}px`;
    return node;
}
/** Runs a Record lens binding, saying whether it had anything to do. */
export function recordAction(view, action) {
    const rows = view.record?.rows ?? [];
    const decision = view.record?.decision;
    const at = rows.findIndex((row) => row.on);
    const selectAt = (i) => {
        const row = rows[Math.max(0, Math.min(rows.length - 1, i))];
        if (row)
            call('select_decision', { id: row.id });
    };
    switch (action) {
        case 'next':
            selectAt(at + 1);
            return true;
        case 'previous':
            selectAt(at - 1);
            return true;
        case 'first':
            selectAt(0);
            return true;
        case 'last':
            selectAt(rows.length - 1);
            return true;
        case 'edit':
            if (decision)
                editDecision(decision);
            return true;
        case 'decide':
            begin({ what: 'addDecision', under: null, task: null, text: '' });
            return true;
        case 'under':
            if (decision)
                begin({ what: 'addDecision', under: decision.id, task: null, text: '' });
            return true;
        default:
            return decision ? decisionAction(decision, action) : false;
    }
}
