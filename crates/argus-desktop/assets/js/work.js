// A feature opened from the index: its tasks and the decisions raised under
// them as one outline, the row under the cursor in full beside it, and the
// edits a person makes to them in place. Every edit is sent as it stands and
// the form closes; what the feature says changes only when the daemon's
// pushed board and tasks do, so a refused edit leaves it as it was.
import { $, el } from './dom.js';
import { actionFor } from './keymap.js';
import { state } from './state.js';
import { call } from './tauri.js';
const TASK_MARK = {
    todo: ['○', 'fg-faint'],
    doing: ['◐', 'fg-attention'],
    done: ['✓', 'fg-ok'],
    open: ['?', 'fg-attention'],
    accepted: ['◆', 'fg-ref'],
    rejected: ['✕', 'fg-faint'],
    withdrawn: ['⊘', 'fg-faint'],
    superseded: ['⊘', 'fg-faint'],
};
function edit(change) {
    call('edit_feature', { edit: change });
}
function begin(editing) {
    state.editing = editing;
    drawWork();
}
function cancel() {
    state.editing = null;
    drawWork();
}
/** Sends what the form holds and puts it away; the push redraws. */
function save() {
    const editing = state.editing;
    state.editing = null;
    if (editing)
        send(editing);
    drawWork();
}
function send(editing) {
    switch (editing.what) {
        case 'brief':
            edit({ on: 'brief', body: editing.text });
            break;
        case 'task':
            edit({ on: 'task', id: editing.id, title: editing.title, body: editing.body });
            break;
        case 'record':
            edit({ on: 'rewrite', id: editing.id, document: editing.text });
            break;
        case 'answer':
            edit({
                on: 'answer',
                id: editing.id,
                answer: editing.text,
                because: editing.because.trim() ? editing.because : null,
            });
            break;
        case 'criterion':
            if (editing.text.trim())
                edit({ on: 'addCriterion', id: editing.id, text: editing.text });
            break;
        case 'addTask':
            if (editing.text.trim())
                edit({ on: 'addTask', parent: editing.parent, title: editing.text });
            break;
        case 'addDecision':
            if (editing.text.trim()) {
                edit({ on: 'addDecision', under: editing.under, task: editing.task, line: editing.text });
            }
            break;
    }
}
/** A text field of the form being written: its keys are the keymap's
 *  Writing place, and what it holds is kept in `state.editing` so a push
 *  that redraws the feature does not lose it. */
function field(id, value, multiline, keep, placeholder) {
    const onkeydown = (e) => {
        const action = actionFor(['writing'], e);
        if (action === 'save' || (action === 'choose' && !multiline)) {
            e.preventDefault();
            save();
        }
        else if (action === 'closeLayer') {
            e.preventDefault();
            cancel();
        }
    };
    const oninput = (e) => {
        const target = e.target;
        if (target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement) {
            keep(target.value);
        }
    };
    const node = multiline
        ? el('textarea', { class: 'field area', id, placeholder, oninput, onkeydown })
        : el('input', { class: 'field', id, placeholder, oninput, onkeydown });
    node.value = value;
    return node;
}
function formButtons(saveLabel) {
    return el('div', { class: 'actions' }, el('button', { class: 'btn primary', onclick: save }, saveLabel, el('span', { class: 'kbd on-accent' }, 'Ctrl Enter')), el('button', { class: 'btn', onclick: cancel }, 'Cancel'));
}
// ---- Drawing -----------------------------------------------------------------
export function drawWork() {
    const view = state.features?.open;
    if (!view)
        return;
    const stage = $('fstage');
    // A redraw replaces the fields; the one being typed in keeps its caret.
    const active = document.activeElement;
    const focused = (active instanceof HTMLInputElement || active instanceof HTMLTextAreaElement) &&
        stage.contains(active)
        ? { id: active.id, start: active.selectionStart, end: active.selectionEnd }
        : null;
    if (!stillThere(view))
        state.editing = null;
    stage.replaceChildren(head(view), el('div', { class: 'split' }, outline(view), detail(view)));
    const again = focused?.id ? document.getElementById(focused.id) : null;
    if (again instanceof HTMLInputElement || again instanceof HTMLTextAreaElement) {
        again.focus();
        again.setSelectionRange(focused?.start ?? null, focused?.end ?? null);
    }
    else {
        const first = stage.querySelector('[data-editing]');
        if (first instanceof HTMLInputElement || first instanceof HTMLTextAreaElement) {
            first.focus();
            first.setSelectionRange(first.value.length, first.value.length);
        }
    }
    stage.querySelector('.wrow.on')?.scrollIntoView({ block: 'nearest' });
}
/** Whether the form being written is still about something on screen. */
function stillThere(view) {
    const editing = state.editing;
    if (!editing || editing.what === 'brief' || editing.what === 'addTask')
        return true;
    const id = editing.what === 'addDecision'
        ? (editing.under ?? editing.task)
        : editing.id;
    return id === null || view.rows.some((row) => row.at.id === id);
}
function head(view) {
    const editing = state.editing;
    const parts = [
        `${view.tasksDone} of ${view.tasksTotal} tasks`,
        view.decided > 0 && el('span', { class: 'fg-ok' }, `${view.decided} decided`),
        view.open > 0 && el('span', { class: 'fg-attention' }, `${view.open} open`),
    ].filter((part) => part !== false);
    const progress = el('span', null);
    progress.style.width = `${view.tasksTotal ? Math.round((view.tasksDone / view.tasksTotal) * 100) : 0}%`;
    let brief;
    if (editing?.what === 'brief') {
        const area = field('edit-brief', editing.text, true, (text) => (editing.text = text), 'What is being built, and why');
        area.dataset.editing = '';
        brief = el('div', { class: 'stack-6 brief-edit' }, area, formButtons('Save the brief'));
    }
    else {
        brief = el('div', { class: 'brief-row' }, view.brief
            ? el('div', { class: 'fg-muted prose' }, view.brief)
            : el('div', { class: 'fg-faint' }, 'No brief yet.'), el('button', { class: 'btn sm', onclick: () => begin({ what: 'brief', text: view.brief }) }, 'Edit brief'));
    }
    return el('div', { class: 'fhead' }, el('div', { class: 'crumbs' }, el('button', { class: 'crumb', onclick: closeFeature }, 'Features'), el('span', { class: 'fg-faint' }, '›'), el('span', { class: 'fg' }, `#${view.number}`), el('span', { class: 'grow' }), el('span', { class: 'meta' }, el('span', { class: 'kbd' }, 'Esc'), ' back to features')), el('div', { class: 'actions head-row' }, el('span', { class: 'type-title' }, el('span', { class: 'mono fg-accent' }, `#${view.number}`), ' ', view.title), el('span', { class: `pill ${view.standing === 'active' ? 'fg-attention' : ''}` }, view.standing), el('span', { class: 'grow' }), el('span', { class: 'meta' }, view.branch && ['branch ', el('span', { class: 'mono fg-ref' }, view.branch)], view.branch && view.agent && ' · ', view.agent && [
        el('span', { class: 'glyph' }, '◆'),
        ` ${view.agent.title} `,
        el('span', { class: `st ${view.agent.status}` }, view.agent.status),
    ])), brief, el('div', { class: 'actions head-row' }, el('span', { class: 'pbar wide' }, progress), el('span', { class: 'meta' }, parts.flatMap((part, i) => (i ? [' · ', part] : [part]))), el('span', { class: 'grow' }), el('span', { class: 'meta' }, keycap('E'), ' edit · ', keycap('A'), ' task · ', keycap('S'), ' under it · ', keycap('D'), ' decision · ', keycap('B'), ' brief')));
}
function keycap(key) {
    return el('span', { class: 'kbd' }, key);
}
function outline(view) {
    const section = el('section', { class: 'work-outline', 'aria-label': 'Outline' }, el('div', { class: 'label' }, el('span', null, 'Outline'), el('span', { class: 'grow' }), el('span', null, 'tasks and the decisions raised under them')));
    if (!view.loaded)
        section.append(el('div', { class: 'meta' }, 'Reading its tasks…'));
    else if (!view.rows.length) {
        section.append(el('div', { class: 'meta' }, 'Nothing written under it yet: add a task, or raise a decision.'));
    }
    for (const row of view.rows)
        section.append(workRow(row));
    const editing = state.editing;
    if (editing?.what === 'addTask' || editing?.what === 'addDecision')
        section.append(adding(view, editing));
    section.append(el('div', { class: 'actions work-foot' }, el('span', { class: 'grow' }), el('button', { class: 'btn', onclick: () => begin({ what: 'addTask', parent: null, text: '' }) }, '+ Task'), el('button', {
        class: 'btn',
        onclick: () => begin({ what: 'addDecision', under: null, task: null, text: '' }),
    }, '+ Decision')));
    return section;
}
function workRow(row) {
    const [mark, tone] = TASK_MARK[row.state] ?? ['·', 'fg-faint'];
    const decision = row.at.kind === 'decision';
    const quiet = row.state === 'done' ||
        row.state === 'withdrawn' ||
        row.state === 'superseded' ||
        row.state === 'rejected';
    let aside = null;
    if (row.state === 'open')
        aside = el('span', { class: 'pill fg-attention' }, 'needs you');
    else if (row.waitsOn)
        aside = el('span', { class: 'meta fg-danger' }, `waits on ${row.waitsOn}`);
    else if (row.state === 'withdrawn')
        aside = el('span', { class: 'meta' }, 'withdrawn');
    const node = el('button', {
        class: `wrow${row.on ? ' on' : ''}`,
        'aria-current': row.on ? 'true' : null,
        onclick: () => call('select_work', { at: row.at }),
        ondblclick: () => editRow(),
    }, el('span', { class: tone }, mark), el('span', {
        class: `n${decision && row.state !== 'open' ? ' fg-ref' : ''}${row.state === 'open' ? ' fg-attention' : ''}`,
    }, row.label), el('span', { class: `${row.on ? 'strong' : ''} ${quiet ? 'fg-muted' : ''}` }, row.text), aside);
    node.style.paddingLeft = `${8 + row.depth * 20}px`;
    return node;
}
/** The one-line field adding a task or a decision, and what it adds to. */
function adding(view, editing) {
    const label = (id) => view.rows.find((row) => row.at.id === id)?.label ?? '';
    let what;
    let placeholder;
    if (editing.what === 'addTask') {
        what = editing.parent === null ? 'New task' : `New subtask under ${label(editing.parent)}`;
        placeholder = 'What is to be done, in a line';
    }
    else {
        if (editing.under !== null)
            what = `Sub-decision under ${label(editing.under)}`;
        else if (editing.task !== null)
            what = `Decision raised under task ${label(editing.task)}`;
        else
            what = 'Decision on the feature';
        placeholder = '? a question still open, ~ an answer turned down, or what was decided';
    }
    const input = field('edit-add', editing.text, false, (text) => (editing.text = text), placeholder);
    input.dataset.editing = '';
    return el('div', { class: 'stack-4 work-add' }, el('div', { class: 'label' }, what), input, formButtons('Add'));
}
function detail(view) {
    if (view.task)
        return taskDetail(view, view.task);
    if (view.decision)
        return decisionDetail(view.decision);
    return el('article', { class: 'work-detail' }, el('div', { class: 'meta' }, 'Choose a task or a decision to read it here.'));
}
function titled(label, ...body) {
    return el('div', { class: 'stack-4' }, el('div', { class: 'label' }, label), body);
}
function lineList(lines, mark, tone) {
    return lines.map((line) => el('div', null, el('span', { class: tone }, `${mark} `), el('span', { class: 'mono fg-ref' }, line.label), ' ', el('span', { class: 'fg-muted' }, line.text)));
}
function taskDetail(view, task) {
    const editing = state.editing;
    const stateWord = {
        todo: '○ not started',
        doing: '◐ in progress',
        done: '✓ done',
    };
    const top = el('div', null, el('div', { class: 'label' }, el('span', { class: 'fg-accent' }, `Task ${task.label}`), task.parent && el('span', { class: 'label-aside' }, `under ${task.parent}`)), el('div', { class: 'type-title detail-title' }, task.title), el('div', { class: 'meta' }, el('span', { class: task.state === 'doing' ? 'fg-attention' : task.state === 'done' ? 'fg-ok' : '' }, stateWord[task.state]), task.waitsOn && el('span', { class: 'fg-danger' }, ` · waits on ${task.waitsOn}`)));
    if (editing?.what === 'task' && editing.id === task.id) {
        const title = field('edit-task-title', editing.title, false, (text) => (editing.title = text), 'The task, in a line');
        title.dataset.editing = '';
        const body = field('edit-task-body', editing.body, true, (text) => (editing.body = text), 'What it is, and anything the one doing it should know');
        return el('article', { class: 'work-detail', 'aria-label': `Task ${task.label}` }, el('div', { class: 'label fg-accent' }, `Editing task ${task.label}`), titled('Title', title), titled('Description', body), formButtons('Save the task'));
    }
    const criteria = task.criteria.map((criterion, index) => el('div', { class: 'crit' }, el('label', { class: 'check' }, el('input', {
        type: 'checkbox',
        checked: criterion.done,
        onchange: () => edit({ on: 'setCriterion', id: task.id, index, done: !criterion.done }),
    }), el('span', { class: criterion.done ? 'fg-muted' : '' }, criterion.text)), el('button', {
        class: 'icon sm',
        'aria-label': `Remove “${criterion.text}”`,
        title: 'Remove',
        onclick: () => edit({ on: 'removeCriterion', id: task.id, index }),
    }, '×')));
    let addCriterion = null;
    if (editing?.what === 'criterion' && editing.id === task.id) {
        const input = field('edit-criterion', editing.text, false, (text) => (editing.text = text), 'One checkable line');
        input.dataset.editing = '';
        addCriterion = el('div', { class: 'stack-4' }, input, formButtons('Add'));
    }
    return el('article', { class: 'work-detail', 'aria-label': `Task ${task.label}` }, top, titled('Description', task.body
        ? el('div', { class: 'fg-muted prose' }, task.body)
        : el('div', { class: 'fg-faint' }, 'No description.')), titled('Done when', criteria.length
        ? criteria
        : !addCriterion && el('div', { class: 'fg-faint' }, 'Nothing says when it is done yet.'), addCriterion), task.boundBy.length > 0 && titled('Bound by', lineList(task.boundBy, '✓', 'fg-ok')), task.notes.length > 0 &&
        titled('Notes', task.notes.map((note) => el('div', null, el('span', { class: 'meta' }, note.label), el('div', { class: 'fg-muted' }, note.text)))), el('div', { class: 'grow' }), el('div', { class: 'actions' }, el('button', { class: 'btn', onclick: () => editRow() }, 'Edit'), el('button', { class: 'btn', onclick: () => begin({ what: 'criterion', id: task.id, text: '' }) }, 'Add criterion'), el('button', { class: 'btn', onclick: () => begin({ what: 'addTask', parent: task.id, text: '' }) }, 'Add subtask'), el('button', {
        class: 'btn',
        onclick: () => begin({ what: 'addDecision', under: null, task: task.id, text: '' }),
    }, 'Raise a decision'), el('span', { class: 'grow' }), view.agent &&
        el('button', { class: 'btn primary', onclick: () => goToPane(view) }, 'Go to pane')));
}
function decisionDetail(decision) {
    const editing = state.editing;
    const open = decision.state === 'open';
    const tone = open ? 'fg-attention' : decision.state === 'accepted' ? 'fg-ok' : 'fg-faint';
    const top = el('div', null, el('div', { class: 'label' }, el('span', { class: `mono ${tone}` }, decision.label), el('span', { class: tone }, decision.state), el('span', { class: 'grow' }), decision.raisedUnder &&
        el('span', { class: 'label-aside' }, `raised under ${decision.raisedUnder}`)), el('div', { class: 'type-title detail-title' }, decision.chose), decision.withdrawn &&
        el('div', { class: 'meta' }, 'Withdrawn: kept on the record, and no longer shown to agents.'));
    if (editing?.what === 'record' && editing.id === decision.id) {
        const area = field('edit-record', editing.text, true, (text) => (editing.text = text), '');
        area.dataset.editing = '';
        area.classList.add('mono', 'tall');
        return el('article', { class: 'work-detail', 'aria-label': `Decision ${decision.label}` }, el('div', { class: 'label fg-accent' }, `Editing ${decision.label} as its record`), area, el('div', { class: 'meta' }, 'The first line is the decision. Each ', el('span', { class: 'mono' }, '## section'), ' is a part of the record; under Consequences, + is a gain and - a cost.'), formButtons('Save the record'));
    }
    if (editing?.what === 'answer' && editing.id === decision.id) {
        const answer = field('edit-answer', editing.text, true, (text) => (editing.text = text), 'What was decided');
        answer.dataset.editing = '';
        const because = field('edit-because', editing.because, false, (text) => (editing.because = text), 'Why, if it needs saying');
        return el('article', { class: 'work-detail', 'aria-label': `Decision ${decision.label}` }, top, titled('Answer', answer), titled('Because', because), el('div', { class: 'meta' }, 'The options not taken stay listed as rejected.'), formButtons('Answer it'));
    }
    const body = [top];
    if (decision.context)
        body.push(titled('Context', el('div', { class: 'fg-muted prose' }, decision.context)));
    if (open)
        body.push(options(decision));
    else {
        if (decision.detail)
            body.push(titled('Decision', el('div', { class: 'fg-muted prose' }, decision.detail)));
        if (decision.because)
            body.push(titled('Why', el('div', { class: 'fg-muted prose' }, decision.because)));
        if (decision.consequences.length) {
            body.push(titled('Consequences', decision.consequences.map((line) => el('div', { class: line.startsWith('+') ? 'fg-ok' : 'fg-attention' }, line))));
        }
        if (decision.over)
            body.push(titled('Considered', el('div', { class: 'fg-muted prose' }, decision.over)));
    }
    if (decision.carriedOutBy.length)
        body.push(titled('Carried out by', lineList(decision.carriedOutBy, '·', 'fg-faint')));
    const chosen = state.chosen?.id === decision.id ? state.chosen.option : null;
    body.push(el('div', { class: 'grow' }), el('div', { class: 'actions' }, open && [
        el('button', {
            class: 'btn primary',
            disabled: chosen === null,
            onclick: () => {
                if (chosen !== null)
                    edit({ on: 'take', id: decision.id, option: chosen });
            },
        }, chosen === null ? 'Take an option' : `Take option ${chosen + 1}`),
        el('button', {
            class: 'btn',
            onclick: () => begin({ what: 'answer', id: decision.id, text: '', because: '' }),
        }, 'Answer in words…'),
    ], el('span', { class: 'grow' }), open && el('span', { class: 'meta' }, 'The others stay listed as rejected.'), el('button', { class: 'btn', onclick: () => editRow() }, 'Edit record'), el('button', {
        class: 'btn',
        onclick: () => begin({ what: 'addDecision', under: decision.id, task: null, text: '' }),
    }, 'Sub-decision'), el('button', {
        class: 'btn',
        onclick: () => edit({ on: 'withdraw', id: decision.id, withdrawn: !decision.withdrawn }),
    }, decision.withdrawn ? 'Restore' : 'Withdraw')));
    return el('article', { class: 'work-detail', 'aria-label': `Decision ${decision.label}` }, body);
}
/** An open decision's options as cards to choose between. */
function options(decision) {
    if (!decision.options.length) {
        return titled('Options', el('div', { class: 'meta' }, 'No options listed: answer it in words.'));
    }
    return el('div', { class: 'stack-8', role: 'radiogroup', 'aria-label': 'Options' }, el('div', { class: 'label' }, 'Options'), decision.options.map((option, index) => {
        const on = state.chosen?.id === decision.id && state.chosen.option === index;
        return el('label', { class: `option${on ? ' on' : ''}` }, el('input', {
            type: 'radio',
            name: `decision-${decision.id}`,
            checked: on,
            onchange: () => {
                state.chosen = { id: decision.id, option: index };
                drawWork();
            },
        }), el('span', { class: 'stack-2' }, el('span', { class: 'meta' }, `Option ${index + 1}`), el('span', { class: 'strong' }, option)));
    }));
}
// ---- Keys --------------------------------------------------------------------
function closeFeature() {
    state.editing = null;
    call('open_feature', { slug: null });
}
function goToPane(view) {
    if (view.agent)
        call('go_to_pane', { pane: view.agent.pane });
}
/** Opens what the cursor is on to be edited: a task's title and
 *  description, a decision as its record, else the brief. */
function editRow() {
    const view = state.features?.open;
    if (!view)
        return;
    if (view.task)
        begin({ what: 'task', id: view.task.id, title: view.task.title, body: view.task.body });
    else if (view.decision)
        begin({ what: 'record', id: view.decision.id, text: view.decision.document });
    else
        begin({ what: 'brief', text: view.brief });
}
export function workOpen() {
    return state.rail.stage === 'feature' && state.features?.open != null;
}
/** Escape on an open feature: the form being written goes first, then the
 *  feature, back to the index. */
export function leaveWork() {
    if (state.editing)
        cancel();
    else
        closeFeature();
}
/** Runs an open feature's binding, saying whether it had anything to do. */
export function workAction(action) {
    const view = state.features?.open;
    if (!view)
        return false;
    const at = view.rows.findIndex((row) => row.on);
    const selectAt = (i) => {
        const row = view.rows[Math.max(0, Math.min(view.rows.length - 1, i))];
        if (row)
            call('select_work', { at: row.at });
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
            selectAt(view.rows.length - 1);
            return true;
        case 'edit':
            editRow();
            return true;
        case 'brief':
            begin({ what: 'brief', text: view.brief });
            return true;
        case 'addTask':
            begin({ what: 'addTask', parent: null, text: '' });
            return true;
        case 'under':
            if (view.task)
                begin({ what: 'addTask', parent: view.task.id, text: '' });
            else if (view.decision)
                begin({ what: 'addDecision', under: view.decision.id, task: null, text: '' });
            return true;
        case 'decide':
            begin({ what: 'addDecision', under: null, task: view.task?.id ?? null, text: '' });
            return true;
        case 'answer':
            if (view.decision?.state === 'open') {
                begin({ what: 'answer', id: view.decision.id, text: '', because: '' });
            }
            return true;
        case 'withdraw':
            if (view.decision) {
                edit({ on: 'withdraw', id: view.decision.id, withdrawn: !view.decision.withdrawn });
            }
            return true;
        case 'goToPane':
            goToPane(view);
            return true;
        default:
            return false;
    }
}
