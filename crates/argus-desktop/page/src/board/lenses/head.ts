// An open feature's header, shared by the Work, Record and Flows lenses: one
// quiet line under the window's title (its state, how far it is, its tickets
// and its brief), and the tools that sit on the title's own line (the lenses
// and a menu for what is rarer). The words are `head_words.ts`'s.

import type { FeatureView } from '../../bindings/FeatureView';
import { el, icon, standingPill } from '../../kit/dom.js';
import { openMenu, type MenuItem } from '../../kit/menu.js';
import { featureViewItem, savedFeatureItems } from '../tracker/tickets.js';
import { call } from '../../connection/tauri.js';
import { state } from '../../state.js';
import { begin, field, formButtons, LENSES, setLens } from './work.js';
import { reconcileEntry, ticketsLabel } from './head_words.js';

/** The count a lens carries as its suffix: what it has to show. */
function lensCount(view: FeatureView, lens: (typeof LENSES)[number]['lens']): number | null {
  if (lens === 'work') return view.tasksTotal;
  if (lens === 'record') return view.decided + view.open;
  return view.flows?.loaded ? view.flows.flows.length : null;
}

/** The line under the title. The brief takes what room the rest leaves and
 *  is the way into editing it, so the header carries no Edit button. */
export function statusLine(view: FeatureView): HTMLElement {
  const editing = state.editing;
  if (editing?.what === 'brief') {
    const area = field(
      'edit-brief',
      editing.text,
      true,
      (text) => (editing.text = text),
      'What is being built, and why',
    );
    return el(
      'div',
      { class: 'fhead' },
      el('div', { class: 'stack-6 brief-edit' }, area, formButtons('Save the brief')),
    );
  }
  const tally = [
    `${view.tasksDone} of ${view.tasksTotal} tasks`,
    view.decided > 0 && `${view.decided} decided`,
    view.open > 0 && `${view.open} open`,
  ].filter((part) => part !== false);
  const progress = el('span', null);
  progress.style.width = `${view.tasksTotal ? Math.round((view.tasksDone / view.tasksTotal) * 100) : 0}%`;
  const tickets = ticketsLabel(view.links);
  return el(
    'div',
    { class: 'fhead' },
    el(
      'div',
      { class: 'head-line' },
      standingPill(view.standing),
      view.tasksTotal > 0 &&
        el(
          'span',
          { class: 'meta head-progress', title: tally.join(' · ') },
          el('span', { class: 'pbar wide' }, progress),
          `${view.tasksDone} of ${view.tasksTotal}`,
        ),
      tickets !== null &&
        el(
          'button',
          {
            class: 'btn sm',
            id: 'feature-tickets',
            type: 'button',
            title: 'The linked tickets (T)',
            onclick: () => call('open_link', { map: false }),
          },
          tickets,
        ),
      el(
        'button',
        {
          class: `brief-line${view.brief ? '' : ' fg-faint'}`,
          type: 'button',
          title: view.brief ? `${view.brief}\n\nEdit the brief (B)` : 'Write the brief (B)',
          onclick: () => begin({ what: 'brief', text: view.brief }),
        },
        el('span', { class: 'brief-label' }, 'Brief'),
        view.brief || 'No brief yet.',
      ),
    ),
  );
}

/** What the title's line carries for an open feature: the lenses switch
 *  what it is shown as, so they are segments, not tabs. */
export function featureTools(view: FeatureView): HTMLElement[] {
  const seg = el(
    'div',
    { class: 'seg', role: 'radiogroup', 'aria-label': 'Lens' },
    LENSES.map(({ lens, name, key }) =>
      el(
        'button',
        {
          class: `segb${view.lens === lens ? ' on' : ''}`,
          role: 'radio',
          'aria-checked': String(view.lens === lens),
          title: `${name} (${key})`,
          onclick: () => setLens(lens),
        },
        name,
        lensCount(view, lens) !== null &&
          el('span', { class: 'fg-faint' }, ` ${lensCount(view, lens)}`),
      ),
    ),
  );
  const views: MenuItem[] = [...view.views.map(featureViewItem), ...savedFeatureItems(view.number)];
  const entry = view.links.length > 0 ? reconcileEntry(view) : null;
  const more = entry?.attention ? 'Feature menu: Reconcile, a proposal waits' : 'Feature menu';
  const items = (): (MenuItem | false)[] => [
    { label: 'Edit the brief', hint: 'B', run: () => begin({ what: 'brief', text: view.brief }) },
    { label: 'Link a ticket…', hint: 'T', run: () => call('open_link', { map: false }) },
    entry !== null && {
      label: entry.label,
      hint: entry.hint,
      run: () => call('open_reconcile', {}),
    },
    // The window's Views button holds every view; these are the ones that name this feature.
    views.length > 0 && 'sep',
    ...views,
  ];
  const tools: HTMLElement[] = [seg];
  tools.push(
    el(
      'button',
      {
        class: 'icon menu-dots',
        type: 'button',
        'aria-label': more,
        title: more,
        onclick: (e: MouseEvent) => {
          const at = (e.currentTarget as HTMLElement).getBoundingClientRect();
          openMenu('Feature menu', items(), { x: at.right, y: at.bottom + 4, align: 'end' });
        },
      },
      icon('more'),
      entry?.attention && el('span', { class: 'attn-dot', 'aria-hidden': 'true' }),
    ),
  );
  return tools;
}
