// The composer: what to start, where, and the agent's first message. The
// repository is the one chosen on the rail, so picking another chooses it
// there; the checkout, the agent and the message are a draft the page
// keeps until a pane it started is made.

import { $, el, icon } from '../kit/dom.js';
import { chordsFor, keycaps } from '../keys/chords.js';
import { actionFor } from '../keys/keymap.js';
import { state } from '../state.js';
import { call } from '../connection/tauri.js';

type Repository = NonNullable<typeof state.rail.project>['repositories'][number];

/** Built once and only refilled after, so a redraw for a pane's status
 *  never takes the caret out of the message. */
let parts: {
  title: HTMLElement;
  text: HTMLTextAreaElement;
  repository: HTMLSelectElement;
  checkout: HTMLSelectElement;
  agent: HTMLSelectElement;
  shell: HTMLButtonElement;
  start: HTMLButtonElement;
  where: HTMLElement;
  keys: HTMLElement;
} | null = null;

function repository(): Repository | null {
  const repos = state.rail.project?.repositories ?? [];
  return repos.find((repo) => repo.id === state.rail.repository) ?? repos[0] ?? null;
}

/** The draft's checkout while its repository lists it, else the one on
 *  screen there, else the repository's first: its own working directory. */
function checkout(repo: Repository | null): Repository['checkouts'][number] | null {
  const checkouts = repo?.checkouts ?? [];
  const shown = state.rail.shown;
  return (
    checkouts.find((co) => co.id === state.compose.checkout) ??
    checkouts.find(
      (co) => shown && shown.repository === repo?.name && co.name === shown.checkout,
    ) ??
    checkouts[0] ??
    null
  );
}

function template(): string | null {
  const { templates } = state.rail;
  return templates.find((name) => name === state.compose.template) ?? templates[0] ?? null;
}

function start(agent: boolean): void {
  const co = checkout(repository());
  const name = template();
  if (!co || (agent && !name)) return;
  const prompt = state.compose.text.trim();
  call('compose', {
    checkout: co.id,
    template: agent ? name : null,
    prompt: agent && prompt ? state.compose.text : null,
  });
}

function pick(label: string, glyph: Node | string, select: HTMLSelectElement): HTMLElement {
  return el(
    'label',
    { class: 'pick' },
    el('span', { class: 'pick-glyph' }, glyph),
    el('span', { class: 'sr-only' }, label),
    select,
  );
}

function build(): NonNullable<typeof parts> {
  const text = el('textarea', {
    id: 'compose-text',
    rows: '4',
    placeholder: 'Describe the work, or leave it empty to start the agent idle',
    oninput: () => {
      state.compose.text = text.value;
    },
    onkeydown: (e: KeyboardEvent) => {
      if (actionFor(['writing'], e) === 'save') {
        e.preventDefault();
        start(true);
      }
    },
  });
  const repositorySelect = el('select', {
    onchange: () => {
      const id = Number(repositorySelect.value);
      state.compose.checkout = null;
      if (id !== state.rail.repository) call('choose_repository', { repository: id });
    },
  });
  const checkoutSelect = el('select', {
    onchange: () => {
      state.compose.checkout = Number(checkoutSelect.value);
      drawComposer();
    },
  });
  const agentSelect = el('select', {
    onchange: () => {
      state.compose.template = agentSelect.value;
    },
  });
  const shell = el('button', { class: 'btn', onclick: () => start(false) }, 'Shell');
  const startButton = el(
    'button',
    { class: 'btn primary', onclick: () => start(true) },
    'Start agent',
    el('span', { class: 'kbd on-accent' }, 'Ctrl Enter'),
  );
  const title = el('div', { class: 'compose-title' });
  const where = el('div', { class: 'compose-where' });
  const keys = el('span', { class: 'compose-keys' });
  $('nstage').replaceChildren(
    el(
      'div',
      { class: 'compose' },
      where,
      title,
      el(
        'section',
        { class: 'composer', 'aria-label': 'Start an agent' },
        el('label', { for: 'compose-text', class: 'sr-only' }, 'First message'),
        text,
        el(
          'div',
          { class: 'compose-row' },
          pick('Repository', icon('folder-sm'), repositorySelect),
          pick('Checkout', icon('branch'), checkoutSelect),
          pick('Agent', '◆', agentSelect),
          el('span', { class: 'grow' }),
          shell,
          startButton,
        ),
      ),
      el(
        'div',
        { class: 'compose-foot' },
        el(
          'span',
          { class: 'compose-note' },
          icon('info'),
          'The message becomes the agent’s first prompt. Shell starts a shell there and leaves it.',
        ),
        keys,
      ),
    ),
  );
  return {
    title,
    text,
    repository: repositorySelect,
    checkout: checkoutSelect,
    agent: agentSelect,
    shell,
    start: startButton,
    where,
    keys,
  };
}

/** Puts `values` in a select as its options, `chosen` selected, leaving it
 *  alone when nothing changed so an open list stays open. */
function fill(select: HTMLSelectElement, values: [string, string][], chosen: string | null): void {
  const same =
    select.options.length === values.length &&
    values.every(
      ([value, label], i) =>
        select.options[i]?.value === value && select.options[i]?.text === label,
    );
  if (!same) {
    select.replaceChildren(...values.map(([value, label]) => el('option', { value }, label)));
  }
  if (chosen !== null) select.value = chosen;
  select.disabled = values.length === 0;
}

export function drawComposer(): void {
  $('nstage').hidden = state.rail.stage !== 'new';
  if (state.rail.stage !== 'new') return;
  parts ??= build();
  const repo = repository();
  const co = checkout(repo);
  const name = template();
  const repos = state.rail.project?.repositories ?? [];
  parts.title.textContent = repo
    ? `What should start in ${repo.name}?`
    : 'No repository to start in';
  fill(
    parts.repository,
    repos.map((r) => [String(r.id), r.name]),
    repo ? String(repo.id) : null,
  );
  fill(
    parts.checkout,
    (repo?.checkouts ?? []).map((c) => [
      String(c.id),
      c.branch && c.branch !== c.name ? `${c.name} · ${c.branch}` : c.name,
    ]),
    co ? String(co.id) : null,
  );
  fill(
    parts.agent,
    state.rail.templates.map((t) => [t, t]),
    name,
  );
  if (parts.text.value !== state.compose.text) parts.text.value = state.compose.text;
  parts.shell.disabled = !co;
  parts.start.disabled = !co || !name;
  const keys = chordsFor('compose', 'anywhere');
  parts.where.replaceChildren(
    el('span', { class: 'compose-rule' }),
    !repo
      ? 'Add a repository to the project to start here.'
      : !co
        ? `${repo.name} is folded on the rail; open it to pick a checkout.`
        : el(
            'span',
            null,
            co.kind === 'main' ? 'Starting in ' : 'Starting in the worktree ',
            el('b', { class: 'fg' }, co.kind === 'main' ? repo.name : co.name),
            co.branch ? [' on ', el('span', { class: 'mono fg' }, co.branch)] : '',
          ),
    el('span', { class: 'compose-rule' }),
  );
  parts.keys.replaceChildren(
    ...(keys.length > 0 ? [keycaps(keys), ' opens this from anywhere'] : []),
  );
}

/** The message takes the keys when the composer comes up. */
export function focusComposer(): void {
  parts?.text.focus();
}

/** A start the daemon made is done with: the next composer starts empty. */
export function clearComposer(): void {
  state.compose.text = '';
  if (parts) parts.text.value = '';
}
