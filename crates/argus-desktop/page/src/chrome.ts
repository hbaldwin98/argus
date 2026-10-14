// The window's frame around the stage: the rail, the stage tabs, the pane
// header, the status bar, the empty stage, and notices that pass.

import { $, el, icon, type Child } from './dom.js';
import { drawFeatures } from './features.js';
import { screen } from './screen.js';
import { state } from './state.js';
import { call } from './tauri.js';

const LOUD = new Set(['waiting', 'needs-review', 'failed']);

export function drawChrome(): void {
  const stopped = state.status.connection === 'stopped';
  const feature = state.rail.stage === 'feature';
  for (const [id, on] of [
    ['tab-workspace', !feature],
    ['tab-feature', feature],
  ] as const) {
    $(id).classList.toggle('on', on);
    $(id).setAttribute('aria-selected', String(on));
  }
  $('fstage').hidden = !feature;
  $('body').hidden = feature;
  drawFeatures();
  drawTabsMeta();
  drawHeading();
  drawRail();
  drawHead();
  drawBar();
  drawStage();
  // What the window last heard, dimmed: the heading, the rail and the
  // pane's header as much as its screen.
  for (const id of ['heading', 'rail', 'head']) $(id).classList.toggle('held', stopped);
  $('stopped').hidden = !stopped;
}

/** The Feature stage names the repository it reads, at the tabs' end. */
function drawTabsMeta(): void {
  const { project, repository, stage } = state.rail;
  const chosen = project?.repositories.find((repo) => repo.id === repository);
  $('tabs-meta').textContent = stage === 'feature' && chosen ? chosen.name : '';
}

export function drawHeading(): void {
  const { project, projects } = state.rail;
  const heading = $('heading');
  if (!project) {
    heading.replaceChildren();
    return;
  }
  const several = projects.length > 1;
  const button = el(
    'button',
    {
      class: 'projbtn',
      'aria-label': several ? 'Switch project' : null,
      'aria-expanded': several ? String(state.menuOpen) : null,
      onclick: () => {
        if (!several) return;
        state.menuOpen = !state.menuOpen;
        drawHeading();
      },
    },
    el('span', null, project.name),
    several && icon('down'),
    project.needsYou > 0 && el('span', { class: 'badge' }, `${project.needsYou} need you`),
  );
  const menu =
    several &&
    state.menuOpen &&
    el(
      'div',
      { class: 'menu', role: 'menu' },
      projects.map((choice) =>
        el(
          'button',
          {
            class: `mi${choice.id === project.id ? ' on' : ''}`,
            role: 'menuitem',
            onclick: () => {
              state.menuOpen = false;
              call('select_project', { project: choice.id });
              drawHeading();
            },
          },
          el('span', { class: 'grow' }, choice.name),
          choice.needsYou > 0 && el('span', { class: 'st waiting' }, `${choice.needsYou}`),
        ),
      ),
    );
  heading.replaceChildren(button, menu || '');
}

function statusColour(word: string | null): string | null {
  if (word && LOUD.has(word)) return 'var(--attention)';
  if (word === 'working') return 'var(--accent)';
  return null;
}

function drawRail(): void {
  const rail = $('rail');
  const { project, loaded } = state.rail;
  if (!loaded) {
    rail.replaceChildren(
      el(
        'div',
        { class: 'skeleton', 'aria-hidden': 'true' },
        // A repository, its checkout, two panes, another checkout.
        [
          [110, null],
          [160, 'var(--sp-24)'],
          [190, 'var(--sp-32)'],
          [140, 'var(--sp-32)'],
          [120, 'var(--sp-24)'],
        ].map(([width, indent]) => {
          const ghost = el('div', { class: 'ghost' });
          ghost.style.width = `${width}px`;
          if (indent) ghost.style.marginLeft = String(indent);
          return ghost;
        }),
      ),
    );
    return;
  }
  if (!project) {
    rail.replaceChildren(el('div', { class: 'empty-rail' }, 'No projects yet.'));
    return;
  }
  const rows: HTMLElement[] = [];
  for (const repo of project.repositories) {
    if (repo.divider) rows.push(el('div', { class: 'rule', 'aria-hidden': 'true' }));
    const colour = statusColour(repo.status);
    const statusDot = colour ? el('span', { class: 'rdot' }) : null;
    if (statusDot && colour) statusDot.style.background = colour;
    const chosen = repo.id === state.rail.repository;
    rows.push(
      el(
        'div',
        { class: `repo${repo.quiet ? ' quiet' : ''}${chosen ? ' sel' : ''}` },
        el(
          'button',
          {
            class: 'repobtn',
            'aria-expanded': String(repo.open),
            'aria-current': chosen ? 'true' : null,
            onclick: () => call('choose_repository', { repository: repo.id }),
          },
          el('span', { class: 'chev' }, icon(repo.open ? 'down' : 'right')),
          statusDot,
          el('span', { class: 'rname ttl' }, repo.name),
          el('span', { class: 'cnt' }, repo.used ?? (repo.panes || '')),
        ),
      ),
    );
    for (const co of repo.checkouts) {
      rows.push(
        el(
          'div',
          { class: 'co' },
          el('span', { class: 'name', title: co.branch || null }, co.name),
          el('span', null, co.kind),
          co.git != null && el('span', { class: `git${co.clean ? ' clean' : ''}` }, co.git),
          el(
            'button',
            {
              class: 'icon',
              title: 'Start a shell',
              'aria-label': `Start a shell on ${repo.name} ${co.name}`,
              onclick: () => call('spawn_shell', { checkout: co.id }),
            },
            icon('plus'),
          ),
        ),
      );
      for (const pane of co.panes) {
        rows.push(
          el(
            'button',
            {
              class: `pane${pane.on ? ' on' : ''} ${pane.status}`,
              title: pane.note,
              'aria-current': pane.on ? 'true' : null,
              onclick: () => {
                call('select', { pane: pane.id });
                $('input').focus();
              },
            },
            el('span', { class: `glyph${pane.agent ? '' : ' sh'}` }, pane.agent ? '◆' : '›'),
            el('span', { class: 'ttl' }, pane.title),
            el('span', { class: `st ${pane.status}` }, pane.status),
          ),
        );
      }
    }
  }
  rail.replaceChildren(...rows);
}

function drawHead(): void {
  const shown = state.rail.shown;
  const head = $('head');
  head.hidden = !shown || state.rail.stage === 'feature';
  if (!shown) return;
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
    el('span', { class: 'type-heading head-title' }, shown.title),
    el('span', { class: `pill ${shown.status}` }, shown.status),
    el('span', { class: 'grow' }),
    el('span', { class: 'meta mono', id: 'size' }, screen.sizeLabel()),
  );
}

function dot(colour?: string): HTMLElement {
  const node = el('span', { class: 'dot' });
  if (colour) node.style.background = colour;
  return node;
}

function drawBar(): void {
  const { connection, alarm } = state.status;
  const { needsYou, working } = state.rail;
  const left: HTMLElement[] = [];
  const right: HTMLElement[] = [];
  if (connection === 'connected') {
    if (needsYou > 0) {
      left.push(el('span', { class: 'fg-attention strong' }, `▲ ${needsYou} need you`));
    }
    if (working > 0) left.push(el('span', { class: 'fg-accent' }, `⋮ ${working} working`));
    if (alarm) left.push(el('span', { class: 'fg-attention' }, alarm));
    right.push(el('span', { class: 'actions' }, dot(), 'Connected'));
  } else if (connection === 'connecting') {
    left.push(el('span', { class: 'actions' }, dot('var(--attention)'), 'Connecting'));
    left.push(el('span', { class: 'fg-faint' }, 'retrying through argus bridge'));
  } else {
    left.push(el('span', { class: 'actions' }, dot('var(--fg-faint)'), 'Stopped'));
    left.push(el('span', { class: 'fg-faint' }, 'no reconnect until you ask'));
    right.push(el('button', { class: 'btn sm', onclick: reconnect }, 'Reconnect'));
  }
  $('bar').replaceChildren(...left, el('span', { class: 'grow' }), ...right);
}

function drawStage(): void {
  const { connection, detail } = state.status;
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
      icon('spinner'),
      el('div', { class: 'type-title' }, 'Connecting to argusd…'),
      detail &&
        el(
          'div',
          { class: 'fg-muted' },
          'The last try ended before the daemon said a word. This is what ',
          el('span', { class: 'fg mono' }, 'argus bridge'),
          ' printed:',
        ),
      detail && el('div', { class: 'err' }, detail),
      el('div', { class: 'meta' }, 'Trying again every 2 seconds.'),
    ];
  } else if (!project) {
    measure = 'no-projects';
    body = [
      icon('folder'),
      el('div', { class: 'type-title' }, 'No projects'),
      el(
        'div',
        { class: 'fg-muted' },
        'Add a folder of repositories, or run this in one, and it shows up here.',
      ),
      el('div', { class: 'cmd' }, 'argus init'),
    ];
  } else {
    measure = 'pick';
    body = [
      icon('terminal'),
      el('div', { class: 'type-title' }, 'Pick a pane on the left'),
      el(
        'div',
        { class: 'fg-muted' },
        'Its screen shows here and takes your keys. To start a shell, use + on a checkout.',
      ),
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
