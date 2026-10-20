// The Flows lens of an open feature: its flows and the selected one's steps
// on the left, with the step under the cursor read whole under them, and
// the sequence diagram drawn on the right (`sequence.ts`), which expands
// over the window. A binding, a step's text and a flow's source are each
// sent as the flow rewritten, and show when the daemon's pushed flows do.

import type { Action } from '../../bindings/Action';
import type { FeatureView } from '../../bindings/FeatureView';
import type { FlowDetail } from '../../bindings/FlowDetail';
import type { StepRow } from '../../bindings/StepRow';
import { closeLayer, placeMenu } from '../../kit/menu.js';
import {
  clickable,
  drawSequence,
  type Drawn,
  editSource,
  editStep,
  openExpanded,
  sourceForm,
} from './sequence.js';
import { el, icon, type Child } from '../../kit/dom.js';
import { state } from '../../state.js';
import { call } from '../../connection/tauri.js';
import { begin, edit, field, formButtons, keycap, lineList, overflow, titled } from './work.js';

export function flowsLens(view: FeatureView): HTMLElement {
  const flows = view.flows;
  const flow = flows?.flow ?? null;
  const editing = state.editing;
  const list = el(
    'section',
    { class: 'work-outline flows-list', 'aria-label': 'Flows' },
    el(
      'div',
      { class: 'label' },
      el('span', null, 'Flows'),
      el('span', { class: 'grow' }),
      el('span', null, String(flows?.flows.length ?? 0)),
    ),
  );
  if (!flows?.loaded) list.append(el('div', { class: 'meta' }, 'Reading its flows…'));
  else if (!flows.flows.length) {
    list.append(
      el(
        'div',
        { class: 'meta' },
        'No flows yet. Add one from a skeleton, or an agent records one with ',
        el('span', { class: 'mono' }, 'argus-hook diagram'),
        '.',
      ),
    );
  }
  for (const item of flows?.flows ?? []) {
    list.append(
      el(
        'button',
        {
          class: `frow${item.on ? ' on' : ''}`,
          'aria-current': item.on ? 'true' : null,
          onclick: () => selectStep(item.id, null),
        },
        el('span', { class: item.on ? 'strong' : 'fg-muted' }, item.title),
        el('span', { class: 'meta' }, item.summary),
      ),
    );
  }
  if (editing?.what === 'addFlow') {
    list.append(
      el(
        'div',
        { class: 'stack-4 work-add' },
        el('div', { class: 'label' }, 'New flow'),
        field('edit-add', editing.text, false, (text) => (editing.text = text), 'What it shows'),
        formButtons('Add'),
      ),
    );
  }
  if (flow) {
    list.append(
      el('div', { class: 'label steps-label' }, el('span', null, 'Steps')),
      el('div', { class: 'stack-2 steps-list' }, flowSteps(flow)),
      el('div', { class: 'step-below' }, stepBelow(flow)),
    );
  }
  list.append(
    el(
      'div',
      { class: 'actions work-foot' },
      el('span', { class: 'grow' }),
      el('button', { class: 'btn', onclick: () => begin({ what: 'addFlow', text: '' }) }, '+ Flow'),
    ),
  );
  return el('div', { class: 'split' }, list, picture(flow));
}

/** A flow's steps as the list draws them, with each branch between. */
export function flowSteps(flow: FlowDetail): HTMLElement[] {
  return flow.rows.map((row) => stepRow(flow, row));
}

function stepRow(flow: FlowDetail, row: StepRow): HTMLElement {
  const number = row.number;
  if (number === null) {
    return el(
      'div',
      { class: 'steprow branch' },
      el('span', null),
      el('span', { class: 'fg-attention mono' }, row.route),
      el('span', { class: 'fg-muted' }, row.text),
    );
  }
  return el(
    'button',
    {
      class: `steprow${row.on ? ' on' : ''}`,
      'aria-current': row.on ? 'true' : null,
      onclick: () => selectStep(flow.id, number - 1),
    },
    el('span', { class: `n ${row.on ? 'fg-accent' : ''}` }, String(number)),
    el('span', { class: 'mono fg-muted route' }, row.route),
    el(
      'span',
      { class: 'wtext' },
      el('span', { class: row.on ? 'strong' : '' }, row.text),
      row.tags.map((tag) => el('span', { class: 'tkey' }, tag)),
    ),
  );
}

/** Under the steps: the one under the cursor and what stands behind it, or
 *  the form editing it or its flow. */
function stepBelow(flow: FlowDetail): Child {
  const editing = state.editing;
  // Expanded, the forms are the panel's.
  if (state.expanded) return null;
  if (editing?.what === 'flow' && editing.id === flow.id) return sourceForm(editing);
  const step = flow.step;
  if (!step) return null;
  if (editing?.what === 'step' && editing.diagram === flow.id && editing.step === step.number) {
    return el(
      'div',
      { class: 'stack-6 step-detail' },
      titled(
        `Step ${step.number} says`,
        field(
          'edit-step',
          editing.text,
          false,
          (text) => (editing.text = text),
          'what is asked or answered',
        ),
      ),
      formButtons('Save the step'),
    );
  }
  const nothing = !step.boundBy.length && !step.carriedOutBy.length;
  return el(
    'div',
    { class: 'stack-8 step-detail' },
    nothing && el('div', { class: 'meta' }, `Nothing binds step ${step.number} yet.`),
    step.boundBy.length > 0 &&
      titled(`Step ${step.number} is bound by`, lineList(step.boundBy, '✓', 'fg-ok')),
    step.carriedOutBy.length > 0 &&
      titled('Carried out by', lineList(step.carriedOutBy, '·', 'fg-faint')),
    el(
      'div',
      { class: 'actions detail-foot' },
      el('span', { class: 'grow' }),
      el('button', { class: 'btn sm', onclick: () => editStep(flow) }, 'Edit step'),
      el(
        'button',
        { class: 'btn sm', onclick: (e: MouseEvent) => bindMenu(flow, e) },
        'Bind a decision…',
      ),
      overflow('More: edit source', [{ label: 'Edit source', run: () => editSource(flow) }]),
    ),
  );
}

function picture(flow: FlowDetail | null): HTMLElement {
  if (!flow) {
    return el(
      'article',
      { class: 'work-detail' },
      el('div', { class: 'meta' }, 'Choose a flow to draw it here.'),
    );
  }
  const drawn = drawSequence(flow.diagram);
  clickable(drawn, flow);
  return el(
    'article',
    { class: 'work-detail flow-picture', 'aria-label': flow.title },
    el(
      'div',
      { class: 'label' },
      el('span', null, flow.title),
      el('span', { class: 'grow' }),
      el(
        'button',
        { class: 'btn', onclick: openExpanded, title: 'Expand over the window' },
        icon('expand'),
        ' Expand ',
        keycap('F'),
      ),
    ),
    fitted(el('div', { class: 'seq-holder' }, drawn.node), drawn),
  );
}

/** The diagram scaled down, no further than 85%, so a few lifelines fit
 *  the box; when it still overflows, the selected step is brought into view
 *  and both edges fade as far as there is more to scroll to. */
function fitted(holder: HTMLElement, drawn: Drawn): HTMLElement {
  const update = (): void => {
    const more = holder.scrollWidth - holder.clientWidth - holder.scrollLeft > 4;
    holder.classList.toggle('scrolls', more);
    holder.classList.toggle('scrolled', holder.scrollLeft > 0);
    holder.title = more || holder.scrollLeft > 0 ? 'Scroll sideways for the rest' : '';
  };
  holder.addEventListener('scroll', update, { passive: true });
  requestAnimationFrame(() => {
    // Minus the holder's padding and border, which it also scrolls inside.
    const room = holder.clientWidth - 24;
    if (room <= 0) return;
    const scale = Math.max(0.85, Math.min(1, room / drawn.width));
    drawn.node.setAttribute('width', String(drawn.width * scale));
    drawn.node.setAttribute('height', String(drawn.height * scale));
    const lit = holder.querySelector('.smsg.on');
    if (lit && holder.scrollWidth > holder.clientWidth) {
      const at = lit.getBoundingClientRect();
      const box = holder.getBoundingClientRect();
      const centre = at.left - box.left + holder.scrollLeft + at.width / 2;
      holder.scrollLeft = Math.max(0, centre - holder.clientWidth / 2);
    }
    update();
  });
  return holder;
}

/** Puts the cursor on a flow and, given one, a step of it, counted from
 *  zero; with none the flow keeps the step it was on. */
export function selectStep(diagram: number, step: number | null): void {
  call('select_step', { diagram, step });
}

/** The decisions a step can be bound to, as a menu by the button or the
 *  step; choosing one writes its comment under the step. */
export function bindMenu(flow: FlowDetail, at: { clientX: number; clientY: number } | null): void {
  const step = flow.step;
  const decisions = state.features?.open?.flows?.decisions ?? [];
  if (!step) return;
  const menu = el(
    'div',
    { class: 'menu', role: 'menu', 'aria-label': `Bind step ${step.number}` },
    el('div', { class: 'mlabel' }, `Bind step ${step.number} to`),
    decisions.length === 0 &&
      el('div', { class: 'mi', 'aria-disabled': 'true' }, 'No decision to bind it to yet.'),
    decisions.map((decision) => {
      const bound = step.boundBy.some((line) => line.label === decision.label);
      return el(
        'button',
        {
          class: 'mi',
          role: 'menuitem',
          disabled: bound,
          onclick: () => {
            closeLayer();
            const id = Number(decision.aside?.replace('#', ''));
            if (Number.isFinite(id)) {
              edit({ on: 'bindStep', diagram: flow.id, step: step.number, decision: id });
            }
          },
        },
        el('span', { class: 'mono fg-ref' }, decision.label),
        el('span', { class: 'grow' }, decision.text),
        bound && el('span', { class: 'meta' }, 'bound'),
      );
    }),
  );
  placeMenu(menu, {
    x: at?.clientX ?? window.innerWidth / 2 - 140,
    y: at?.clientY ?? window.innerHeight / 3,
  });
}

/** Runs a Flows lens binding, saying whether it had anything to do. */
export function flowsAction(view: FeatureView, action: Action): boolean {
  const flow = view.flows?.flow;
  switch (action) {
    case 'addTask':
      begin({ what: 'addFlow', text: '' });
      return true;
    case 'nextFlow':
    case 'previousFlow':
    case 'next':
    case 'previous':
    case 'first':
    case 'last':
      call('feature_action', { action });
      return true;
    default:
      break;
  }
  if (!flow) return false;
  switch (action) {
    case 'expand':
    case 'choose':
      openExpanded();
      return true;
    case 'edit':
      editSource(flow);
      return true;
    case 'decide': {
      const lit = document.querySelector('#fstage .steprow.on');
      const box = lit?.getBoundingClientRect();
      bindMenu(flow, box ? { clientX: box.left + 40, clientY: box.bottom } : null);
      return true;
    }
    default:
      return false;
  }
}
