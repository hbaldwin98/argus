// The window's frame around the stage: the sidebar and its nav, the strip's
// words, the sidebar's foot, the empty stage, and notices that pass.

import type { Action } from './bindings/Action';
import type { Resumed } from './bindings/Resumed';
import type { StatusWord } from './bindings/StatusWord';
import { drawCheckouts } from './checkouts/stage.js';
import { drawCode } from './code/stage.js';
import { clock } from './kit/clock.js';
import { drawComposer } from './agents/compose.js';
import { drawConsole } from './panes/console.js';
import { $, el, ghosts, icon, type Child } from './kit/dom.js';
import { drawExtensionNav, drawExtensionPage, extensionPageTitle } from './extensions/views.js';
import { drawSavedNav } from './views/saved.js';
import { drawFeatures } from './board/feature_stage.js';
import { chordsFor, keycaps, presses } from './keys/chords.js';
import { contextMenu, type MenuItem } from './kit/menu.js';
import { closesOnMiddle } from './kit/middle.js';
import { drawHeading, drawRemoving } from './chrome/projects.js';
import { renameDialog } from './panes/rename.js';
import { screen } from './panes/screen.js';
import { dragFromRail, drawSplit } from './panes/split.js';
import { openSettings } from './settings/preferences.js';
import { dragEdge } from './layout/edge.js';
import { state } from './state.js';
import { featureEdges } from './board/lenses/work.js';
import { codeEdges } from './code/stage.js';
import { call } from './connection/tauri.js';

const LOUD = new Set<StatusWord>(['waiting', 'needs-review', 'failed']);

export function drawChrome(): void {
  const stopped = state.status.connection === 'stopped';
  railWidth(state.rail.width);
  featureEdges(state.rail.outlineWidth, state.rail.briefHeight);
  codeEdges(state.rail.codeTreeWidth, state.rail.codeOutlineWidth);
  const stage = state.rail.stage;
  $('fstage').hidden = stage !== 'feature';
  $('cstage').hidden = stage !== 'checkouts';
  $('kstage').hidden = stage !== 'code';
  $('xstage').hidden = stage !== 'extension';
  $('vstage').hidden = stage !== 'view';
  $('body').hidden = !state.rail.workspaceShown;
  drawFeatures();
  drawCheckouts();
  drawCode();
  drawExtensionPage();
  drawComposer();
  drawSplit();
  drawConsole();
  drawNav();
  drawHeading();
  drawRemoving();
  drawRail();
  drawHead();
  drawBar();
  drawStage();
  // What the window last heard, dimmed: the heading, the rail and the
  // pane's header as much as its screen.
  for (const id of ['heading', 'rail', 'head']) $(id).classList.toggle('held', stopped);
  $('stopped').hidden = !stopped;
}

/** The sidebar's way between stages, each with its key from the keymap,
 *  and Search with the palette's; then the extensions' views, a fold of
 *  their own after the built-in stages. */
function drawNav(): void {
  const stage = state.rail.stage;
  for (const [name, action] of [
    ['new', 'compose'],
    ['workspace', 'workspace'],
    ['feature', 'feature'],
    ['checkouts', 'checkouts'],
    ['code', 'code'],
    ['console', 'console'],
  ] as const) {
    const nav = $(`nav-${name}`);
    nav.classList.toggle('on', stage === name);
    if (stage === name) nav.setAttribute('aria-current', 'page');
    else nav.removeAttribute('aria-current');
    $(`nav-${name}-keys`).replaceChildren(keycaps(chordsFor(action, 'anywhere')));
  }
  $('nav-search-keys').replaceChildren(keycaps(chordsFor('palette', 'anywhere')));
  drawExtensionNav();
  drawSavedNav();
}

const STAGE_NAME = {
  feature: 'Features',
  checkouts: 'Checkouts',
  code: 'Code',
  new: 'New',
  console: 'Console',
  extension: 'Extensions',
  view: 'View',
} as const;

/** The repository the Feature and Checkouts stages read. */
function chosenRepository(): string | null {
  const { project, repository } = state.rail;
  return project?.repositories.find((repo) => repo.id === repository)?.name ?? null;
}

function statusColour(word: StatusWord | null): string | null {
  if (word && LOUD.has(word)) return 'var(--attention)';
  if (word === 'working') return 'var(--accent)';
  return null;
}

/** The few words a pane row says beside its dot: only what asks for a
 *  look, or that it has ended. Working shows in the dot alone. */
const PANE_WORD: Partial<Record<StatusWord, string>> = {
  waiting: 'needs you',
  'needs-review': 'review',
  failed: 'failed',
  exited: 'exited',
};

type RailRepository = NonNullable<typeof state.rail.project>['repositories'][number];
type RailCheckout = RailRepository['checkouts'][number];
type RailPane = RailCheckout['panes'][number];

function drawRail(): void {
  const rail = $('rail');
  const { project, loaded } = state.rail;
  if (!loaded) {
    // A repository, its checkout, two panes, another checkout.
    rail.replaceChildren(
      ghosts([
        [110, 16],
        [160, 24],
        [190, 32],
        [140, 32],
        [120, 24],
      ]),
    );
    return;
  }
  if (!project) {
    rail.replaceChildren(el('div', { class: 'empty-rail' }, 'No projects yet.'));
    return;
  }
  const rows: HTMLElement[] = [];
  for (const repo of project.repositories) {
    rows.push(repoRow(repo));
    for (const co of repo.checkouts) {
      rows.push(checkoutRow(repo.name, co));
      if (co.panes.length > 0) {
        rows.push(
          el(
            'div',
            { class: 'nest' },
            co.panes.map((pane) => paneRow(co.id, pane)),
          ),
        );
      }
    }
  }
  rail.replaceChildren(...rows);
}

function repoRow(repo: RailRepository): HTMLElement {
  const colour = statusColour(repo.status);
  const statusDot = colour ? el('span', { class: 'rdot' }) : null;
  if (statusDot && colour) statusDot.style.background = colour;
  const chosen = repo.id === state.rail.repository;
  return el(
    'div',
    {
      class: `repo${repo.quiet ? ' quiet' : ''}${chosen ? ' sel' : ''}`,
      oncontextmenu: contextMenu(repo.name, () => [
        { heading: repo.name },
        {
          label: 'Features',
          run: () => {
            if (!chosen) call('choose_repository', { repository: repo.id });
            call('show_features', { open: true });
          },
        },
        {
          label: 'Recent agent sessions',
          run: () => call('open_sessions', { repository: repo.id }),
        },
        'sep',
        {
          label: repo.open ? 'Collapse' : 'Expand',
          run: () => call('toggle_repository', { repository: repo.id }),
        },
      ]),
    },
    el(
      'button',
      {
        class: 'repobtn',
        'aria-expanded': String(repo.open),
        'aria-current': chosen ? 'true' : null,
        onclick: () => call('choose_repository', { repository: repo.id }),
      },
      el('span', { class: 'chev' }, icon(repo.open ? 'down' : 'right')),
      el('span', { class: 'rname ttl' }, repo.name),
      statusDot,
      el('span', { class: 'cnt' }, repo.used ?? (repo.panes || '')),
    ),
    el(
      'button',
      {
        class: 'icon',
        title: 'Recent agent sessions',
        'aria-label': `Recent agent sessions in ${repo.name}`,
        onclick: () => call('open_sessions', { repository: repo.id }),
      },
      icon('clock'),
    ),
  );
}

function checkoutRow(repository: string, co: RailCheckout): HTMLElement {
  const picked = state.railPick?.checkout === co.id && state.railPick.pane === null;
  const open = railStart(co.id);
  const where = co.kind === 'main' ? 'main checkout' : 'worktree';
  // Folded, the row speaks for the panes it hides: how many, and the
  // loudest of them, so one needing a person is not put out of sight.
  const colour = co.open ? null : statusColour(co.status);
  const hiddenDot = colour ? el('span', { class: 'rdot' }) : null;
  if (hiddenDot && colour) hiddenDot.style.background = colour;
  return el(
    'div',
    {
      class: `co${picked ? ' picked' : ''}${open ? ' hot' : ''}`,
      title: co.branch ? `${where} on ${co.branch}` : where,
      onclick: () => pickRail(co.id, null),
      oncontextmenu: contextMenu(`${repository} ${co.name}`, () => checkoutMenu(co)),
    },
    co.count > 0 &&
      el(
        'button',
        {
          class: 'cofold',
          title: co.open ? 'Fold its panes away' : 'Show its panes',
          'aria-label': `${co.open ? 'Fold' : 'Unfold'} ${repository} ${co.name}`,
          'aria-expanded': String(co.open),
          onclick: (e: MouseEvent) => {
            e.stopPropagation();
            foldCheckout(co.id);
          },
        },
        icon(co.open ? 'down' : 'right'),
      ),
    icon('branch'),
    el('span', { class: 'name ttl' }, co.name),
    co.shared !== null &&
      el(
        'span',
        { class: 'shared', title: `shared by ${co.shared} agents` },
        icon('people'),
        `${co.shared}`,
      ),
    co.git != null && el('span', { class: `git${co.clean ? ' clean' : ''}` }, co.git),
    !co.open &&
      el(
        'span',
        {
          class: 'cofolded',
          title: `${co.count} ${co.count === 1 ? 'pane' : 'panes'} folded away`,
        },
        hiddenDot,
        `${co.count}`,
      ),
    el(
      'button',
      {
        class: `icon${open ? ' hot' : ''}`,
        title: 'Start a shell or an agent',
        'aria-label': `Start a shell or an agent on ${repository} ${co.name}`,
        'aria-expanded': String(open),
        onclick: (e: MouseEvent) => {
          e.stopPropagation();
          toggleStart(co.id, 'rail');
        },
      },
      icon('plus'),
    ),
    open && startMenu(`${repository} ${co.name}`, co.id),
  );
}

function checkoutMenu(co: RailCheckout): MenuItem[] {
  return [
    { heading: co.branch || co.name },
    ...starts().map((item) => ({
      label: item.agent ? `New agent · ${item.agent}` : 'New shell',
      run: () => start(co.id, item.agent),
    })),
    'sep',
    { label: 'Review changes', run: () => call('open_review', { checkout: co.id }) },
    { label: 'History', run: () => call('open_history', { checkout: co.id }) },
    { label: 'Show in Checkouts', run: () => call('go_to_checkout', { checkout: co.id }) },
    ...(co.count > 0
      ? [{ label: co.open ? 'Collapse' : 'Expand', run: () => foldCheckout(co.id) }]
      : []),
    ...(co.kind === 'worktree'
      ? ([
          'sep',
          {
            label: 'Remove worktree…',
            danger: true,
            run: () => call('remove_checkout', { checkout: co.id }),
          },
        ] as MenuItem[])
      : []),
  ];
}

/** What a pane's row, its tab or the stage's header offers. */
export function paneMenu(pane: number, title: string, first: MenuItem[] = []): MenuItem[] {
  return [
    { heading: title },
    ...first,
    { label: 'Rename…', run: () => renameDialog(pane, title) },
    'sep',
    { label: 'Close', danger: true, run: () => call('close', { pane }) },
  ];
}

function paneRow(checkout: number, pane: RailPane): HTMLElement {
  const word = PANE_WORD[pane.status];
  const picked = state.railPick?.pane === pane.id;
  const row = el(
    'div',
    {
      class: 'pane-row',
      oncontextmenu: contextMenu(pane.title, () =>
        paneMenu(pane.id, pane.title, [
          {
            label: 'Show',
            run: () => {
              pickRail(checkout, pane.id);
              call('select', { pane: pane.id });
            },
          },
          {
            label: 'Open beside',
            run: () => call('open_beside', { pane: pane.id }),
          },
        ]),
      ),
    },
    el(
      'button',
      {
        class: `pane${pane.on ? ' on' : ''}${picked ? ' picked' : ''}${pane.agent ? '' : ' sh'} ${pane.status}`,
        title: pane.note ?? `${pane.agent ? 'agent' : 'shell'}, ${pane.status}`,
        'aria-current': pane.on ? 'true' : null,
        // Dragged onto the stage it opens there, beside what is shown.
        onmousedown: (e: MouseEvent) => dragFromRail(e, pane.id),
        // The pane comes on screen, but the keys stay in the rail
        // until Enter or a click on the terminal, so x closes it.
        onclick: () => {
          pickRail(checkout, pane.id);
          call('select', { pane: pane.id });
        },
      },
      el('span', { class: `sdot ${pane.status}`, 'aria-hidden': 'true' }),
      el('span', { class: 'ttl' }, pane.title),
      pane.views > 0 &&
        el(
          'span',
          {
            class: 'pane-views',
            title: `${pane.views} ${pane.views === 1 ? 'view' : 'views'} from this agent`,
          },
          `${pane.views}`,
        ),
      el(
        'span',
        { class: `st ${pane.status}` },
        word ?? el('span', { class: 'sr-only' }, pane.status),
      ),
    ),
    closeButton(pane.id, pane.title),
  );
  return closesOnMiddle(row, () => call('close', { pane: pane.id }));
}

// ---- The rail's picked row -------------------------------------------------

function pickRail(checkout: number, pane: number | null): void {
  state.railPick = { checkout, pane };
  $('input').blur();
  drawRail();
}

/** Folds a checkout's panes away or back. A pane picked under it gives
 *  the pick to the checkout, since its row is about to go. */
function foldCheckout(checkout: number): void {
  if (state.railPick?.checkout === checkout && state.railPick.pane !== null) {
    state.railPick = { checkout, pane: null };
  }
  call('toggle_checkout', { checkout });
}

export function railPicked(): boolean {
  return state.railPick !== null;
}

export function unpickRail(): void {
  if (!state.railPick) return;
  state.railPick = null;
  drawRail();
}

/** Moves the pick `by` rows, through every checkout and its panes in the
 *  rail's order, stopping at the ends. A pane picked comes on screen, as
 *  a click on it does. */
function stepRail(by: number): void {
  const pick = state.railPick;
  if (!pick) return;
  const rows = (state.rail.project?.repositories ?? []).flatMap((repo) =>
    repo.checkouts.flatMap((co) => [
      { checkout: co.id, pane: null as number | null },
      ...co.panes.map((pane) => ({ checkout: co.id, pane: pane.id })),
    ]),
  );
  const at = rows.findIndex((row) => row.checkout === pick.checkout && row.pane === pick.pane);
  const next = rows[Math.max(0, Math.min(rows.length - 1, at + by))];
  if (!next) return;
  pickRail(next.checkout, next.pane);
  if (next.pane !== null) call('select', { pane: next.pane });
}

/** Runs `action` on the rail's picked row, saying whether it took it. */
export function railAction(action: Action): boolean {
  const pick = state.railPick;
  if (!pick) return false;
  switch (action) {
    case 'closeLayer':
      unpickRail();
      return true;
    case 'next':
    case 'previous':
      stepRail(action === 'next' ? 1 : -1);
      return true;
    case 'startShell':
      start(pick.checkout, null);
      return true;
    case 'startAgent': {
      const agent = state.rail.templates[0];
      if (agent) start(pick.checkout, agent);
      else notice('argusd offers no agent to start');
      return true;
    }
    case 'openReview':
      call('open_review', { checkout: pick.checkout });
      return true;
    case 'openHistory':
      call('open_history', { checkout: pick.checkout });
      return true;
    case 'fold':
      foldCheckout(pick.checkout);
      return true;
    case 'close':
      // On a checkout, the Checkouts stage opens on it and asks first.
      if (pick.pane === null) {
        call('remove_checkout', { checkout: pick.checkout });
        return true;
      }
      call('close', { pane: pick.pane });
      // The rail drops the pane when the daemon's tree does; the cursor
      // falls back to its checkout.
      pickRail(pick.checkout, null);
      return true;
    case 'choose':
      if (pick.pane === null) {
        toggleStart(pick.checkout, 'rail');
        return true;
      }
      unpickRail();
      $('input').focus();
      return true;
    default:
      return false;
  }
}

// ---- A checkout's + menu ---------------------------------------------------

// What the + menu offers, in its order: a shell, then each of the daemon's
// agent templates, the first its default.
function starts(): { label: string; agent: string | null }[] {
  return [
    { label: 'Shell', agent: null },
    ...state.rail.templates.map((name) => ({ label: `Agent · ${name}`, agent: name })),
  ];
}

function start(checkout: number, agent: string | null): void {
  closeStart();
  // The new pane takes the keys when it comes on screen.
  unpickRail();
  if (agent === null) call('spawn_shell', { checkout });
  else call('spawn_agent', { checkout, template: agent });
}

function railStart(checkout: number): boolean {
  return state.start?.checkout === checkout && state.start.from === 'rail';
}

/** Opens a checkout's + menu where it was pressed, or closes it. */
export function toggleStart(checkout: number, from: 'rail' | 'table'): void {
  const open = state.start?.checkout === checkout && state.start.from === from;
  state.start = open ? null : { checkout, from, at: 0 };
  drawRail();
  drawCheckouts();
}

export function startMenu(name: string, checkout: number): HTMLElement {
  const items = starts();
  const at = state.start?.at ?? 0;
  return el(
    'div',
    { class: 'menu', role: 'menu', 'aria-label': `Start on ${name}` },
    items.flatMap((item, i) => [
      i === 1 && el('div', { class: 'msep' }),
      el(
        'button',
        {
          class: `mi${i === at ? ' on' : ''}`,
          role: 'menuitem',
          onclick: (e: MouseEvent) => {
            e.stopPropagation();
            start(checkout, item.agent);
          },
        },
        el('span', { class: `glyph${item.agent ? '' : ' sh'}` }, item.agent ? '◆' : '›'),
        el('span', { class: 'grow' }, item.label),
        i < 2 && keycaps(chordsFor(i === 0 ? 'startShell' : 'startAgent', 'start').slice(0, 1)),
      ),
    ]),
  );
}

export function startOpen(): boolean {
  return state.start !== null;
}

export function closeStart(): void {
  if (!state.start) return;
  state.start = null;
  drawRail();
  drawCheckouts();
}

/** Runs an open + menu's `action`, saying whether it took it. */
export function startAction(action: Action): boolean {
  const open = state.start;
  if (!open) return false;
  const items = starts();
  switch (action) {
    case 'closeLayer':
      closeStart();
      return true;
    case 'startShell':
      start(open.checkout, null);
      return true;
    case 'startAgent': {
      const agent = state.rail.templates[0];
      if (agent) start(open.checkout, agent);
      else notice('argusd offers no agent to start');
      return true;
    }
    case 'next':
    case 'previous':
      open.at = (open.at + (action === 'next' ? 1 : items.length - 1)) % items.length;
      drawRail();
      drawCheckouts();
      return true;
    case 'choose': {
      const item = items[open.at];
      if (item) start(open.checkout, item.agent);
      return true;
    }
    default:
      return false;
  }
}

/** The strip's words: the pane on screen, or the stage and the
 *  repository it reads. */
function drawHead(): void {
  const { shown } = state.rail;
  // Fullscreen, the console is what the strip names, whichever stage is
  // under it.
  const stage = state.console?.place === 'fullscreen' ? 'console' : state.rail.stage;
  const head = $('head');
  if (stage !== 'workspace') {
    const repository = stage === 'console' || stage === 'extension' ? null : chosenRepository();
    head.replaceChildren(
      el(
        'span',
        { class: 'type-heading' },
        stage === 'extension' ? extensionPageTitle() : STAGE_NAME[stage],
      ),
      repository
        ? el('span', { class: 'crumbs' }, el('span', { class: 'fg-faint' }, '·'), repository)
        : '',
    );
    return;
  }
  if (!shown) {
    head.replaceChildren();
    return;
  }
  head.replaceChildren(
    el(
      'span',
      { class: 'crumbs' },
      el('span', null, shown.repository),
      el('span', { class: 'fg-faint' }, '/'),
      el('span', { class: 'mono fg-ref' }, shown.branch || shown.checkout),
      el('span', { class: 'fg-faint' }, '/'),
    ),
    el('span', { class: `glyph${shown.agent ? '' : ' sh'}` }, shown.agent ? '◆' : '›'),
    el(
      'span',
      {
        class: 'type-heading head-title',
        oncontextmenu: contextMenu(shown.title, () => paneMenu(shown.pane, shown.title)),
      },
      shown.title,
    ),
    el('span', { class: `pill ${shown.status}` }, shown.status),
    shown.resumed ? el('span', { class: 'meta ellipsis' }, resumedFrom(shown.resumed)) : '',
    el('span', { class: 'grow' }),
    el('span', { class: 'meta mono', id: 'size' }, screen.sizeLabel()),
    closeButton(shown.pane, shown.title),
  );
}

/** What a pane this window resumed a session in was resumed from. */
function resumedFrom(from: Resumed): string {
  const at = clock(from.at);
  const turns =
    from.turns === null ? null : `${from.turns} turn${from.turns === 1 ? '' : 's'} before`;
  return [at ? `resumed from ${at}` : 'resumed', turns].filter((part) => part).join(' · ');
}

/** Ends the pane's program; the rail drops it when the daemon's tree does. */
function closeButton(pane: number, title: string): HTMLElement {
  return el(
    'button',
    {
      class: 'icon close',
      title: 'Close',
      'aria-label': `Close ${title}`,
      onclick: () => call('close', { pane }),
    },
    icon('close'),
  );
}

function dot(colour?: string): HTMLElement {
  const node = el('span', { class: 'dot' });
  if (colour) node.style.background = colour;
  return node;
}

/** The sidebar's foot: the tallies that need a look, a daemon's alarm,
 *  and the connection with the chime and the settings beside it. */
function drawBar(): void {
  const { connection, alarm, host } = state.status;
  const { needsYou, working } = state.rail;
  // A remote daemon is named, so what the rail lists is never taken for this machine's.
  const where = host ? ` to ${host}` : '';
  const tallies: HTMLElement[] = [];
  const line: HTMLElement[] = [];
  if (connection === 'connected') {
    if (needsYou > 0) {
      tallies.push(el('span', { class: 'fg-attention strong' }, `▲ ${needsYou} need you`));
    }
    if (working > 0) tallies.push(el('span', { class: 'fg-accent' }, `⋮ ${working} working`));
    line.push(
      dot(),
      el('span', { class: 'grow ellipsis', title: host ?? '' }, `Connected${where}`),
      consoleChip(),
      chimeSwitch(),
      settingsButton(),
    );
  } else if (connection === 'connecting') {
    line.push(
      dot('var(--attention)'),
      el('span', { class: 'grow ellipsis', title: host ?? '' }, `Connecting${where}`),
      settingsButton(),
    );
  } else {
    line.push(
      dot('var(--fg-faint)'),
      el('span', { class: 'grow' }, 'Stopped'),
      el('button', { class: 'btn sm', onclick: reconnect }, 'Reconnect'),
      settingsButton(),
    );
  }
  const shown = connection === 'connected' && alarm;
  $('bar').replaceChildren(
    tallies.length > 0 ? el('div', { class: 'bar-row' }, tallies) : '',
    shown ? el('div', { class: 'bar-alarm fg-attention' }, alarm) : '',
    el('div', { class: 'bar-row' }, line),
  );
}

/** The console's way back when hidden, and its count of consoles. */
function consoleChip(): HTMLElement {
  const page = state.console;
  const shown = !!page && page.place !== 'hidden';
  const count = page?.tabs.length ?? 0;
  const keys = chordsFor('toggleConsole', 'anywhere')
    .map((chord) => presses(chord).join(' '))
    .join(', ');
  const words = shown ? 'Hide the console' : 'Show the console';
  return el(
    'button',
    {
      class: `chip${shown ? ' on' : ''}`,
      title: keys ? `${words} (${keys})` : words,
      'aria-pressed': String(shown),
      onclick: () => call('console', { gesture: { on: 'toggle' } }),
    },
    el('span', { class: 'mono' }, '›_'),
    count > 0 ? String(count) : '',
  );
}

function chimeSwitch(): HTMLElement {
  const on = state.status.chime;
  const label = on ? 'Mute the chime when an agent needs you' : 'Chime when an agent needs you';
  return el(
    'button',
    {
      class: 'icon',
      title: label,
      'aria-label': label,
      'aria-pressed': String(on),
      onclick: () => call('set_chime', { on: !on }),
    },
    icon(on ? 'bell' : 'bell-off'),
  );
}

/** The way to the settings, named with its key from the keymap. */
function settingsButton(): HTMLElement {
  const keys = chordsFor('settings', 'anywhere')
    .map((chord) => presses(chord).join(' '))
    .join(', ');
  return el(
    'button',
    {
      class: 'icon',
      title: keys ? `Settings (${keys})` : 'Settings',
      'aria-label': 'Settings',
      onclick: openSettings,
    },
    icon('gear'),
  );
}

function drawStage(): void {
  const { connection, detail, host } = state.status;
  const { loaded, project, shown } = state.rail;
  const empty = $('empty');
  $('terminal').hidden = !shown;
  $('body').classList.toggle('held', connection === 'stopped');
  empty.hidden = !!shown;
  if (shown) return;

  let body: Child[];
  // Each state's text is set to its own measure.
  let measure: string;
  if (!loaded) {
    measure = 'connecting';
    body = [
      ghosts([[0.6], [0.9], [0.75], [0.85], [0.4]]),
      el(
        'div',
        { class: 'meta' },
        host ? `Connecting to argusd on ${host}…` : 'Connecting to argusd…',
      ),
      detail && el('div', { class: 'err' }, detail),
      // Nothing is loaded to open the switcher from, so a host that cannot be
      // reached needs its own way back.
      host &&
        el(
          'button',
          { class: 'btn', onclick: () => call('set_host', { host: null }) },
          'Use this machine',
        ),
    ];
  } else if (!project) {
    measure = 'no-projects';
    body = [
      icon('folder'),
      el('div', { class: 'type-title' }, 'No projects'),
      el(
        'button',
        { class: 'btn primary', onclick: () => call('open_add_project', {}) },
        'Add a project…',
      ),
      el('div', { class: 'fg-muted' }, 'or run this in a folder of repositories:'),
      el('div', { class: 'cmd' }, 'argus init'),
    ];
  } else {
    measure = 'no-pane';
    body = [
      icon('terminal'),
      el('div', { class: 'type-title' }, 'Pick a pane'),
      el('div', { class: 'fg-muted' }, 'or start one with + on a checkout.'),
    ];
  }
  empty.replaceChildren(el('div', { class: `empty-body ${measure}` }, body));
}

let noticeTimer: ReturnType<typeof setTimeout> | undefined;

/** Shows a line along the bottom of the window for a few seconds. */
export function notice(text: string): void {
  const node = $('notice');
  node.textContent = text;
  node.hidden = false;
  clearTimeout(noticeTimer);
  noticeTimer = setTimeout(() => {
    node.hidden = true;
  }, 6000);
}

export function reconnect(): void {
  call('reconnect', {});
}

// ---- The rail's width ------------------------------------------------------

/** The width the person dragged the rail to, or the stylesheet's own,
 *  which narrows in a small window, when they never did. */
function railWidth(width: number | null): void {
  const root = document.documentElement.style;
  if (width === null) root.removeProperty('--rail-w');
  else root.setProperty('--rail-w', `${width}px`);
}

/** The rail's right edge, for Rust to hold to its bounds, keep and send
 *  back; a double-click gives back the usual width. */
export function listenToRailEdge(): void {
  const rail = $('rail-sash').parentElement;
  if (!rail) return;
  dragEdge($('rail-sash'), rail, {
    sizeAt: (x) => x - rail.getBoundingClientRect().left,
    draw: railWidth,
    report: (width) => call('rail_width', { width }),
  });
}
