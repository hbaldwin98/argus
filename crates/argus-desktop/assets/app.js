// The desktop window's page: it draws what Rust sends and reports what the
// person does. It decides nothing about the daemon — the model is the
// window's `session` — and holds a mirror of the screen's rows only so a
// theme or scale change can repaint without asking for them again.
'use strict';

const { invoke, Channel } = window.__TAURI__.core;

// The terminal's type, as the design system's --type-mono gives it.
const FONT_SIZE = 13;
const LINE_HEIGHT = 20;

const BOLD = 1, ITALIC = 2, UNDERLINE = 4, REVERSE = 8, WIDE = 16;

const $ = (id) => document.getElementById(id);

const ICON = {
  down: '<svg width="12" height="12" viewBox="0 0 14 14" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M3.5 5.5L7 9l3.5-3.5"></path></svg>',
  right: '<svg width="12" height="12" viewBox="0 0 14 14" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M5.5 3.5L9 7l-3.5 3.5"></path></svg>',
  plus: '<svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"><path d="M8 3v10M3 8h10"></path></svg>',
  spinner: '<svg class="fg-accent spinner" width="40" height="40" viewBox="0 0 40 40" aria-hidden="true"><circle cx="20" cy="20" r="15" fill="none" stroke="currentColor" stroke-opacity=".25" stroke-width="3"></circle><path d="M20 5a15 15 0 0 1 15 15" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round"></path></svg>',
  folder: '<svg class="fg-faint" width="40" height="40" viewBox="0 0 40 40" aria-hidden="true"><path d="M5 12h11l3 4h16v16a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2z" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round"></path></svg>',
  terminal: '<svg class="fg-faint" width="40" height="40" viewBox="0 0 40 40" aria-hidden="true"><rect x="4" y="7" width="32" height="26" rx="3" fill="none" stroke="currentColor" stroke-width="2"></rect><path d="M11 16l5 4-5 4M19 25h9" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"></path></svg>',
};

// ---- Talking to Rust ---------------------------------------------------

function call(command, args) {
  return invoke(command, args).catch((err) => console.error(command, err));
}

const state = {
  status: { connection: 'connecting', detail: null, alarm: null },
  rail: { projects: [], project: null, needsYou: 0, working: 0, shown: null, loaded: false, stage: 'workspace' },
  features: null,
  menuOpen: false,
  // The index's own view state: what the filter says, and whether the
  // board's archived column is unfolded.
  filter: '',
  archivedOpen: false,
};

function connect() {
  const events = new Channel();
  // One bad message is logged rather than ending the stream.
  events.onmessage = (event) => {
    try {
      receive(event);
    } catch (err) {
      console.error(err);
    }
  };
  call('connect', { events });
}

function receive(event) {
  switch (event.type) {
    case 'status':
      state.status = event;
      drawChrome();
      break;
    case 'rail': {
      const before = state.rail.shown?.pane;
      state.rail = event;
      drawChrome();
      // A pane newly on screen takes the keys, however it got there: a
      // click on its row, or a shell started from a checkout's +.
      if (event.shown && event.shown.pane !== before) $('input').focus();
      break;
    }
    case 'frame':
      screen.frame(event);
      break;
    case 'clipboard':
      navigator.clipboard?.writeText(event.text).catch(() => {});
      break;
    case 'notice':
      notice(event.text);
      break;
    case 'features':
      state.features = event;
      drawFeatures();
      break;
  }
}

// ---- Building elements -------------------------------------------------

// Text from the daemon — titles, notes, names — only ever goes in as text.
function el(tag, props, ...children) {
  const node = document.createElement(tag);
  for (const [key, value] of Object.entries(props || {})) {
    if (value == null || value === false) continue;
    if (key === 'class') node.className = value;
    else if (key === 'icon') node.innerHTML = ICON[value];
    else if (key.startsWith('on')) node.addEventListener(key.slice(2), value);
    else node.setAttribute(key, value === true ? '' : value);
  }
  for (const child of children.flat(Infinity)) {
    if (child == null || child === false) continue;
    node.append(child instanceof Node ? child : document.createTextNode(String(child)));
  }
  return node;
}

function icon(name) {
  const holder = el('span', { icon: name });
  return holder.firstElementChild;
}

// ---- The rail, the header, the status bar and the empty stage ----------

const LOUD = new Set(['waiting', 'needs-review', 'failed']);

function drawChrome() {
  const stopped = state.status.connection === 'stopped';
  const feature = state.rail.stage === 'feature';
  for (const [id, on] of [['tab-workspace', !feature], ['tab-feature', feature]]) {
    $(id).classList.toggle('on', on);
    $(id).setAttribute('aria-selected', String(on));
  }
  $('fstage').hidden = !feature;
  $('body').hidden = feature;
  drawFeatures();
  drawHeading();
  drawRail();
  drawHead();
  drawBar();
  drawStage();
  $('rail').classList.toggle('held', stopped);
  $('stopped').hidden = !stopped;
}

function drawHeading() {
  const { project, projects } = state.rail;
  const heading = $('heading');
  if (!project) {
    heading.replaceChildren();
    return;
  }
  const several = projects.length > 1;
  const button = el('button', {
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
  project.needsYou > 0 && el('span', { class: 'badge' }, `${project.needsYou} need you`));
  const menu = several && state.menuOpen && el('div', { class: 'menu', role: 'menu' },
    projects.map((choice) => el('button', {
      class: `mi${choice.id === project.id ? ' on' : ''}`,
      role: 'menuitem',
      onclick: () => {
        state.menuOpen = false;
        call('select_project', { project: choice.id });
        drawHeading();
      },
    },
    el('span', { class: 'grow' }, choice.name),
    choice.needsYou > 0 && el('span', { class: 'st waiting' }, `${choice.needsYou}`))));
  heading.replaceChildren(button, menu || '');
}

function statusColour(word) {
  if (LOUD.has(word)) return 'var(--attention)';
  if (word === 'working') return 'var(--accent)';
  return null;
}

function drawRail() {
  const rail = $('rail');
  const { project, loaded } = state.rail;
  if (!loaded) {
    rail.replaceChildren(el('div', { 'aria-hidden': 'true' },
      [110, 160, 190, 140, 120].map((width) => {
        const ghost = el('div', { class: 'ghost' });
        ghost.style.width = `${width}px`;
        return ghost;
      })));
    return;
  }
  if (!project) {
    rail.replaceChildren(el('div', { class: 'meta empty-rail' }, 'No projects yet.'));
    return;
  }
  const rows = [];
  for (const repo of project.repositories) {
    if (repo.divider) rows.push(el('div', { class: 'rule', 'aria-hidden': 'true' }));
    const colour = statusColour(repo.status);
    const dot = colour && el('span', { class: 'rdot' });
    if (dot) dot.style.background = colour;
    const chosen = repo.id === state.rail.repository;
    rows.push(el('div', { class: `repo${repo.quiet ? ' quiet' : ''}${chosen ? ' sel' : ''}` },
      el('button', {
        class: 'repobtn',
        'aria-expanded': String(repo.open),
        'aria-current': chosen ? 'true' : null,
        onclick: () => call('choose_repository', { repository: repo.id }),
      },
      el('span', { class: 'chev' }, icon(repo.open ? 'down' : 'right')),
      dot,
      el('span', { class: 'rname ttl' }, repo.name),
      el('span', { class: 'cnt' }, repo.used ?? (repo.panes || '')))));
    for (const co of repo.checkouts) {
      rows.push(el('div', { class: 'co' },
        el('span', { class: 'name' }, co.name),
        co.branch && co.branch !== co.name && el('span', null, co.branch),
        co.git != null && el('span', { class: `git${co.clean ? ' clean' : ''}` }, co.git),
        el('button', {
          class: 'icon',
          title: 'Start a shell',
          'aria-label': `Start a shell on ${repo.name} ${co.name}`,
          onclick: () => call('spawn_shell', { checkout: co.id }),
        }, icon('plus'))));
      for (const pane of co.panes) {
        rows.push(el('button', {
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
        el('span', { class: `st ${pane.status}` }, pane.status)));
      }
    }
  }
  rail.replaceChildren(...rows);
}

function drawHead() {
  const shown = state.rail.shown;
  const head = $('head');
  head.hidden = !shown || state.rail.stage === 'feature';
  if (!shown) return;
  head.replaceChildren(
    el('span', { class: 'crumbs' },
      el('span', null, shown.repository),
      el('span', { class: 'fg-faint' }, '/'),
      el('span', { class: 'mono fg-ref' }, shown.branch || shown.checkout),
      el('span', { class: 'fg-faint' }, '/')),
    el('span', { class: `glyph${shown.agent ? '' : ' sh'}` }, shown.agent ? '◆' : '›'),
    el('span', { class: 'type-heading head-title' }, shown.title),
    el('span', { class: `pill ${shown.status}` }, shown.status),
    el('span', { class: 'grow' }),
    el('span', { class: 'meta mono', id: 'size' }, screen.sizeLabel()));
}

function drawBar() {
  const { connection, alarm } = state.status;
  const { needsYou, working } = state.rail;
  const dot = (colour) => {
    const node = el('span', { class: 'dot' });
    if (colour) node.style.background = colour;
    return node;
  };
  const left = [];
  const right = [];
  if (connection === 'connected') {
    if (needsYou > 0) left.push(el('span', { class: 'fg-attention strong' }, `▲ ${needsYou} need you`));
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

function drawStage() {
  const { connection, detail } = state.status;
  const { loaded, project, shown } = state.rail;
  const empty = $('empty');
  $('terminal').hidden = !shown;
  $('body').classList.toggle('held', connection === 'stopped');
  empty.hidden = !!shown;
  if (shown) return;

  let body;
  if (!loaded) {
    body = [
      icon('spinner'),
      el('div', { class: 'type-title' }, 'Connecting to argusd…'),
      detail && el('div', { class: 'fg-muted' }, 'The last try ended before the daemon said a word. This is what ',
        el('span', { class: 'fg mono' }, 'argus bridge'), ' printed:'),
      detail && el('div', { class: 'err' }, detail),
      el('div', { class: 'meta' }, 'Trying again every 2 seconds.'),
    ];
  } else if (!project) {
    body = [
      icon('folder'),
      el('div', { class: 'type-title' }, 'No projects'),
      el('div', { class: 'fg-muted' }, 'Add a folder of repositories, or run this in one, and it shows up here.'),
      el('div', { class: 'cmd' }, 'argus init'),
    ];
  } else {
    body = [
      icon('terminal'),
      el('div', { class: 'type-title' }, 'Pick a pane on the left'),
      el('div', { class: 'fg-muted' }, 'Its screen shows here and takes your keys. To start a shell, use + on a checkout.'),
    ];
  }
  empty.replaceChildren(el('div', { class: 'empty-body' }, body));
}

let noticeTimer = null;
function notice(text) {
  const node = $('notice');
  node.textContent = text;
  node.hidden = false;
  clearTimeout(noticeTimer);
  noticeTimer = setTimeout(() => { node.hidden = true; }, 6000);
}

function reconnect() {
  call('reconnect');
}

// ---- The Feature stage ---------------------------------------------------
//
// The index of one repository's features, as a list beside a peek or as a
// board. Where a feature stands is the daemon's tree and board, read by
// Rust; a hold or an archive is asked for and shows when the board comes
// back saying so. Only Held and Archived take a dropped card: the other
// columns follow the work.

const GROUP_LABEL = { active: 'Active', queued: 'Queued', done: 'Done', held: 'Held', archived: 'Archived' };
const GROUP_CLASS = { active: 'fg-attention', done: 'fg-ok' };
const COLUMN_NOTE = {
  queued: 'written down, not started',
  done: 'every task done',
  held: 'set by you',
  archived: 'set by you',
};

function visibleRows() {
  const f = state.features;
  if (!f) return [];
  const query = state.filter.trim().toLowerCase().replace(/^#/, '');
  return f.rows.filter((row) => {
    if (f.layout === 'list' && !f.showFolded && (row.group === 'held' || row.group === 'archived')) return false;
    if (!query) return true;
    return row.title.toLowerCase().includes(query) || String(row.number) === query;
  });
}

function rowBySlug(slug) {
  return state.features?.rows.find((row) => row.slug === slug);
}

function progress(row, width) {
  const bar = el('span', { class: 'pbar' }, el('span'));
  if (width) bar.style.width = `${width}px`;
  const share = row.tasksTotal ? row.tasksDone / row.tasksTotal : 0;
  bar.firstChild.style.width = `${Math.round(share * 100)}%`;
  return bar;
}

function questions(row) {
  if (row.open > 0) return el('span', { class: 'pill fg-attention start' }, `${row.open} open`);
  if (row.decided > 0) return el('span', { class: 'meta fg-ok' }, `${row.decided} decided`);
  return el('span', { class: 'meta fg-faint' }, '—');
}

function agentLine(agent) {
  return [
    el('span', { class: 'glyph' }, '◆'),
    el('span', { class: 'ttl' }, agent.title),
    el('span', { class: `st ${agent.status}` }, agent.status),
  ];
}

// What a row says under its title: why it is where it is.
function rowNote(row) {
  if (row.held) return el('span', null, el('span', { class: 'fg-faint' }, 'held: '), `“${row.held}”`);
  if (row.after.length) {
    return el('span', null, el('span', { class: 'fg-faint' }, 'after '),
      el('span', { class: 'mono' }, row.after.map((n) => `#${n}`).join(', ')));
  }
  if (row.agent) return agentLine(row.agent);
  if (row.standing === 'new') return el('span', { class: 'fg-faint' }, 'nothing written yet');
  if (row.group === 'done') return el('span', { class: 'fg-faint' }, 'every task done · archive it to accept it');
  if (row.group === 'archived') return el('span', { class: 'fg-faint' }, 'archived');
  return null;
}

function drawFeatures() {
  const stage = $('fstage');
  if (state.rail.stage !== 'feature') return;
  const f = state.features;
  if (!f || !f.repository) {
    stage.replaceChildren(el('div', { class: 'empty' }, el('div', { class: 'empty-body' },
      icon('folder'),
      el('div', { class: 'type-title' }, 'No repository'),
      el('div', { class: 'fg-muted' }, 'Choose a repository on the left to read its features.'))));
    return;
  }
  const head = indexHead(f);
  if (!f.loaded) {
    stage.replaceChildren(head, el('div', { class: 'empty' }, el('div', { class: 'empty-body' },
      icon('spinner'), el('div', { class: 'fg-muted' }, `Reading the features in ${f.repository}…`))));
    return;
  }
  if (!f.rows.length) {
    stage.replaceChildren(head, el('div', { class: 'empty' }, el('div', { class: 'empty-body' },
      icon('folder'),
      el('div', { class: 'type-title' }, `No features in ${f.repository} yet`),
      el('div', { class: 'fg-muted' }, 'An agent opens one with ', el('span', { class: 'mono fg' }, 'argus-hook feature open'),
        ', and it shows up here with its tasks and decisions.'))));
    return;
  }
  stage.replaceChildren(head, f.layout === 'board' ? board(f) : list(f));
}

function indexHead(f) {
  const counts = [
    f.active && el('span', { class: 'fg-attention' }, `${f.active} active`),
    f.queued && el('span', null, `${f.queued} queued`),
    f.done && el('span', { class: 'fg-ok' }, `${f.done} done`),
  ].filter(Boolean).flatMap((part, i) => (i ? [' · ', part] : [part]));
  const folded = f.held + f.archived;
  const filter = el('input', {
    class: 'field filter', id: 'feature-filter', placeholder: 'Filter by title or #', 'aria-label': 'Filter features',
    oninput: (e) => {
      state.filter = e.target.value;
      drawFeatures();
      const again = $('feature-filter');
      again.focus();
      again.setSelectionRange(again.value.length, again.value.length);
    },
  });
  filter.value = state.filter;
  const seg = el('div', { class: 'seg', role: 'radiogroup', 'aria-label': 'Layout' },
    ['list', 'board'].map((layout) => el('button', {
      class: `segb${f.layout === layout ? ' on' : ''}`,
      role: 'radio',
      'aria-checked': String(f.layout === layout),
      onclick: () => call('set_layout', { layout }),
    }, layout === 'list' ? 'List' : 'Board')));
  return el('div', { class: 'fhead index-head' },
    el('div', { class: 'crumbs' }, el('span', { class: 'fg' }, 'Features'), el('span', { class: 'grow' }),
      el('span', { class: 'meta' }, el('span', { class: 'kbd' }, 'V'), ' list or board · ', el('span', { class: 'kbd' }, 'M'), ' move to… · ',
        el('span', { class: 'kbd' }, 'G'), ' go to its pane')),
    el('div', { class: 'actions index-tools' },
      el('span', { class: 'type-title' }, `Features in ${f.repository}`),
      el('span', { class: 'meta' }, counts),
      el('span', { class: 'grow' }),
      filter,
      f.layout === 'list' && folded > 0 && el('button', { class: 'btn', onclick: () => call('toggle_folded') },
        f.showFolded ? 'Fold held and archived ' : 'Show held and archived ', el('span', { class: 'fg-faint' }, String(folded))),
      seg));
}

function list(f) {
  const rows = visibleRows();
  const section = el('section', { class: 'index-list', 'aria-label': 'Features' },
    el('div', { class: 'irow label' }, el('span'), el('span', null, 'Feature'), el('span', null, 'Tasks'),
      el('span', null, 'Questions'), el('span', null, 'Checkout')));
  let group = null;
  for (const row of rows) {
    if (row.group !== group) {
      group = row.group;
      section.append(el('div', { class: 'label group-label' }, el('span', { class: GROUP_CLASS[group] }, GROUP_LABEL[group])));
    }
    const quiet = ['done', 'held', 'archived'].includes(row.group);
    section.append(el('button', {
      class: `irow${row.peeked ? ' on' : ''}`,
      'aria-current': row.peeked ? 'true' : null,
      'data-slug': row.slug,
      onclick: () => call('peek', { slug: row.slug }),
      oncontextmenu: (e) => {
        e.preventDefault();
        call('peek', { slug: row.slug });
        moveMenu(row, e.clientX, e.clientY);
      },
    },
    el('span', { class: `n${row.peeked ? ' fg-accent' : ''}` }, `#${row.number}`),
    el('span', { class: `ttl${row.peeked ? ' strong' : ''}${quiet ? ' fg-muted' : ''}` }, row.title),
    row.standing === 'new'
      ? el('span', { class: 'meta fg-faint' }, 'new')
      : row.tasksTotal === 0
        ? el('span', { class: 'meta fg-faint' }, 'no tasks')
        : el('span', { class: 'meta' }, progress(row), ` ${row.tasksDone}/${row.tasksTotal}`),
    questions(row),
    el('span', { class: `mono ${quiet ? 'fg-faint' : 'fg-ref'} small-mono` }, row.branch || '—'),
    el('span', { class: 'sub2' }, rowNote(row))));
  }
  if (!rows.length) section.append(el('div', { class: 'meta group-label' }, 'Nothing matches the filter.'));
  section.append(el('div', { class: 'meta index-foot' },
    'Held and archived features are folded away. A feature\'s standing is read off its tasks and the agents on its checkouts; only holding and archiving are set by hand.'));
  return el('div', { class: 'split' }, section, peekPanel(f.peek));
}

function lines(label, items, mark) {
  if (!items.length) return null;
  return el('div', { class: 'stack-4' }, el('div', { class: `label${mark === '?' ? ' fg-attention' : ''}` }, label),
    items.map((line) => [
      el('div', null, mark && el('span', { class: mark === '?' ? 'fg-attention' : mark === '✓' ? 'fg-ok' : 'fg-faint' }, `${mark} `),
        el('span', { class: 'mono fg-ref' }, line.label), ' ', line.text),
      line.aside && el('div', { class: 'meta indented' }, line.aside),
    ]));
}

function peekPanel(peek) {
  if (!peek) return el('aside', { class: 'peek', 'aria-label': 'Peek' }, el('div', { class: 'meta' }, 'Choose a feature to peek at it.'));
  const row = rowBySlug(peek.slug);
  return el('aside', { class: 'peek', 'aria-label': `Peek at #${peek.number}` },
    el('div', null,
      el('div', { class: 'label' }, el('span', { class: 'fg-accent' }, `#${peek.number}`),
        el('span', { class: GROUP_CLASS[peek.standing] }, peek.standing), el('span', { class: 'grow' }),
        peek.branch && el('span', { class: 'label-aside' }, `from ${peek.branch}`)),
      el('div', { class: 'peek-title' }, peek.title),
      peek.held && el('div', { class: 'fg-attention' }, `held: “${peek.held}”`),
      peek.brief && el('div', { class: 'fg-muted peek-brief' }, peek.brief)),
    peek.links.length > 0 && el('div', { class: 'actions' }, peek.links.map((link) =>
      el('span', { class: 'track', title: link.url }, link.tracker, ' ', el('b', null, link.key)))),
    lines('Needs you', peek.needsYou, '?'),
    lines('In flight', peek.inFlight, '◐'),
    lines('Next up', peek.nextUp ? [peek.nextUp] : [], '○'),
    lines('Last decided', peek.lastDecided ? [peek.lastDecided] : [], '✓'),
    el('div', { class: 'grow' }),
    el('div', { class: 'actions' },
      el('button', { class: 'btn', disabled: !peek.pane, onclick: () => goToPane(peek.pane) }, 'Go to its pane'),
      el('span', { class: 'grow' }),
      row && moveButtons(row)));
}

function moveButtons(row) {
  return [
    row.group === 'held'
      ? el('button', { class: 'btn', onclick: () => call('hold', { slug: row.slug, reason: null }) }, 'Lift the hold')
      : el('button', { class: 'btn', onclick: () => holdDialog(row) }, 'Hold…'),
    row.group === 'archived'
      ? el('button', { class: 'btn', onclick: () => call('archive', { slug: row.slug, archived: false }) }, 'Reopen')
      : el('button', { class: 'btn', onclick: () => call('archive', { slug: row.slug, archived: true }) }, 'Archive'),
  ];
}

function board(f) {
  const rows = visibleRows();
  const column = (group, extra) => {
    const cards = rows.filter((row) => row.group === group);
    const section = el('section', {
      class: `kcol${extra || ''}`,
      'aria-label': `${GROUP_LABEL[group]}, ${cards.length} features`,
      'data-group': group,
    },
    el('div', { class: 'khead' }, el('span', { class: GROUP_CLASS[group] }, GROUP_LABEL[group]),
      el('span', { class: 'fg-faint' }, String(cards.length)), el('span', { class: 'grow' }),
      COLUMN_NOTE[group] && el('span', { class: 'regular meta' }, COLUMN_NOTE[group])));
    const shown = group === 'archived' && !state.archivedOpen ? [] : cards;
    for (const row of shown) section.append(card(row));
    if (group === 'archived' && cards.length) {
      section.append(el('button', { class: 'kmore', onclick: () => { state.archivedOpen = !state.archivedOpen; drawFeatures(); } },
        state.archivedOpen ? 'Fold the archived' : `Show the ${cards.length} archived`));
    }
    return section;
  };
  return el('div', { class: 'board' },
    column('queued'), column('active'), column('done'),
    el('div', { class: 'side' }, column('archived'), column('held'),
      el('div', { class: 'meta' }, 'Queued, Active and Done follow the work: a card moves when a task starts, an agent arrives or the last task closes. You can drag only into Held, which asks for a reason, and Archived.')));
}

function card(row) {
  const node = el('button', {
    class: `kcard${row.peeked ? ' on' : ''}`,
    'data-slug': row.slug,
    onclick: () => {
      if (drag.justDropped) return;
      call('peek', { slug: row.slug });
    },
    oncontextmenu: (e) => {
      e.preventDefault();
      call('peek', { slug: row.slug });
      moveMenu(row, e.clientX, e.clientY);
    },
    onpointerdown: (e) => {
      // Measured now: a board pushed before the drag starts redraws the
      // card, and the old element measures as nowhere.
      if (e.button === 0) drag.press = { slug: row.slug, x: e.clientX, y: e.clientY, node, box: node.getBoundingClientRect() };
    },
  },
  el('span', { class: 'krow' }, el('span', { class: `n${row.peeked ? ' fg-accent' : ''}` }, `#${row.number}`), el('span', { class: 'grow' }),
    row.after.length ? el('span', { class: 'meta' }, 'after ', el('span', { class: 'mono' }, row.after.map((n) => `#${n}`).join(', ')))
      : row.standing === 'new' ? el('span', { class: 'pill fg-faint' }, 'new') : questions(row)),
  el('span', { class: 'kttl' }, row.title),
  row.standing === 'new'
    ? el('span', { class: 'meta' }, 'nothing written yet')
    : row.tasksTotal === 0
      ? el('span', { class: 'meta' }, 'no tasks yet')
      : el('span', { class: 'krow' }, progress(row), el('span', { class: 'meta' }, `${row.tasksDone}/${row.tasksTotal}`),
      el('span', { class: 'grow' }), row.branch && el('span', { class: 'mono fg-ref small-mono' }, row.branch)),
  row.agent && el('span', { class: 'krow meta' }, agentLine(row.agent)),
  row.held && el('span', { class: 'meta' }, `held: “${row.held}”`));
  return node;
}

// A card being dragged, by pointer events rather than HTML drag and drop:
// one behaviour in every webview (WebKitGTK's drop does not reliably
// arrive), and the card under the pointer is drawn as the canvas draws it.
const drag = { press: null, slug: null, ghost: null, over: null, justDropped: false };

function columnAt(x, y) {
  return document.elementFromPoint(x, y)?.closest('.kcol[data-group]');
}

function markColumn(column) {
  if (drag.over && drag.over !== column) drag.over.classList.remove('drop', 'refuse');
  drag.over = column;
  if (!column) return;
  const settable = column.dataset.group === 'held' || column.dataset.group === 'archived';
  column.classList.toggle('drop', settable);
  column.classList.toggle('refuse', !settable);
}

document.addEventListener('pointermove', (e) => {
  const press = drag.press;
  if (!press) return;
  if (!drag.slug) {
    if (Math.hypot(e.clientX - press.x, e.clientY - press.y) < 6) return;
    drag.slug = press.slug;
    const box = press.box;
    press.node = document.querySelector(`.kcard[data-slug="${CSS.escape(press.slug)}"]`) || press.node;
    drag.ghost = press.node.cloneNode(true);
    drag.ghost.classList.add('ghost-card');
    drag.ghost.style.width = `${box.width}px`;
    drag.offset = { x: press.x - box.left, y: press.y - box.top };
    document.body.append(drag.ghost);
    press.node.classList.add('dragging');
  }
  drag.ghost.style.left = `${e.clientX - drag.offset.x}px`;
  drag.ghost.style.top = `${e.clientY - drag.offset.y}px`;
  markColumn(columnAt(e.clientX, e.clientY));
});

document.addEventListener('pointerup', (e) => {
  const press = drag.press;
  drag.press = null;
  if (!press || !drag.slug) return;
  const column = columnAt(e.clientX, e.clientY);
  const slug = drag.slug;
  drag.ghost?.remove();
  document.querySelector(`.kcard[data-slug="${CSS.escape(slug)}"]`)?.classList.remove('dragging');
  markColumn(null);
  drag.slug = null;
  drag.ghost = null;
  // The click that follows the release is the drag's, not a peek.
  drag.justDropped = true;
  setTimeout(() => { drag.justDropped = false; }, 0);
  const row = rowBySlug(slug);
  if (row && column) drop(row, column.dataset.group);
});

// A card dropped on a column. Only the two a person sets take it; the
// rest say why they follow the work instead.
function drop(row, group) {
  if (group === row.group) return;
  if (group === 'held') return holdDialog(row);
  if (group === 'archived') return call('archive', { slug: row.slug, archived: true });
  const move = row.moves.find((m) => m.to === group);
  notice(`${GROUP_LABEL[group]} follows the work${move?.disabled ? `: ${move.disabled}` : ''}.`);
}

function closeLayer() {
  $('layer').replaceChildren();
}

function moveMenu(row, x, y) {
  const menu = el('div', { class: 'menu', role: 'menu', 'aria-label': `#${row.number} ${row.title}` },
    el('div', { class: 'mlabel' }, `#${row.number} ${row.title}`),
    row.agent && el('button', { class: 'mi', role: 'menuitem', onclick: () => { closeLayer(); goToPane(row.agent.pane); } },
      'Go to its pane', el('span', { class: 'grow' }), el('span', { class: 'kbd' }, 'G')),
    el('div', { class: 'msep' }),
    el('div', { class: 'mlabel' }, 'Move to'),
    row.moves.map((move) => el('button', {
      class: 'mi', role: 'menuitem', disabled: !!move.disabled,
      onclick: () => {
        closeLayer();
        if (move.to === 'held') holdDialog(row);
        else if (move.to === 'unhold') call('hold', { slug: row.slug, reason: null });
        else if (move.to === 'archived') call('archive', { slug: row.slug, archived: true });
        else if (move.to === 'reopen') call('archive', { slug: row.slug, archived: false });
      },
    },
    move.disabled === 'here now' && el('span', { class: 'fg-ok' }, '✓'),
    move.label, el('span', { class: 'grow' }),
    move.disabled ? el('span', { class: 'meta' }, move.disabled)
      : el('span', { class: 'kbd' }, move.to === 'archived' ? 'Shift A' : move.to === 'held' || move.to === 'unhold' ? 'H' : ''))));
  const layer = $('layer');
  layer.replaceChildren(menu);
  const box = menu.getBoundingClientRect();
  menu.style.left = `${Math.min(x, window.innerWidth - box.width - 8)}px`;
  menu.style.top = `${Math.min(y, window.innerHeight - box.height - 8)}px`;
  menu.querySelector('.mi:not([disabled])')?.focus();
}

function holdDialog(row) {
  const reason = el('input', { class: 'field', id: 'hold-reason', placeholder: 'until the ratatui bump lands' });
  const hold = () => {
    call('hold', { slug: row.slug, reason: reason.value });
    closeLayer();
  };
  reason.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') hold();
    if (e.key === 'Escape') closeLayer();
  });
  const dialog = el('div', { class: 'scrim', onclick: (e) => { if (e.target === e.currentTarget) closeLayer(); } },
    el('section', { class: 'dialog hold-dialog', role: 'dialog', 'aria-modal': 'true', 'aria-labelledby': 'hold-title' },
      el('div', { class: 'dhead' }, el('div', { class: 'type-title', id: 'hold-title' }, `Hold #${row.number} ${row.title}`)),
      el('div', { class: 'dbody' },
        el('label', { class: 'label', for: 'hold-reason' }, 'Why is it held?'),
        reason,
        el('div', { class: 'meta' }, 'A held feature is folded away until the hold is lifted. Agents read the reason.')),
      el('div', { class: 'dfoot' }, el('span', { class: 'grow' }),
        el('button', { class: 'btn', onclick: closeLayer }, 'Cancel'),
        el('button', { class: 'btn primary', onclick: hold }, 'Hold'))));
  $('layer').replaceChildren(dialog);
  reason.focus();
}

function goToPane(pane) {
  if (pane == null) return;
  call('go_to_pane', { pane });
}

// The stage's keys, heard only while nothing that takes text has focus.
function featureKey(e) {
  if (state.rail.stage !== 'feature' || e.ctrlKey || e.metaKey || e.altKey) return false;
  const rows = visibleRows();
  const at = rows.findIndex((row) => row.peeked);
  const row = rows[at];
  const peekAt = (i) => {
    const next = rows[Math.max(0, Math.min(rows.length - 1, i))];
    if (next) call('peek', { slug: next.slug });
  };
  switch (e.key) {
    case 'j': case 'ArrowDown': peekAt(at + 1); return true;
    case 'k': case 'ArrowUp': peekAt(at - 1); return true;
    case 'Home': peekAt(0); return true;
    case 'End': peekAt(rows.length - 1); return true;
    case 'v': case 'V':
      call('set_layout', { layout: state.features?.layout === 'board' ? 'list' : 'board' });
      return true;
    case 'z': call('toggle_folded'); return true;
    case '/': $('feature-filter')?.focus(); return true;
  }
  if (!row) return false;
  switch (e.key) {
    case 'm': case 'M': {
      const node = document.querySelector(`[data-slug="${CSS.escape(row.slug)}"]`)?.getBoundingClientRect();
      moveMenu(row, node ? node.left + 40 : 200, node ? node.bottom : 200);
      return true;
    }
    case 'h': case 'H':
      if (row.group === 'held') call('hold', { slug: row.slug, reason: null });
      else holdDialog(row);
      return true;
    case 'A': call('archive', { slug: row.slug, archived: row.group !== 'archived' }); return true;
    case 'g': case 'G': goToPane(row.agent?.pane ?? state.features.peek?.pane); return true;
  }
  return false;
}

document.addEventListener('keydown', (e) => {
  if (e.key === 'Escape' && $('layer').childElementCount) {
    closeLayer();
    e.preventDefault();
    return;
  }
  const typing = e.target.closest?.('input, textarea, [contenteditable]');
  if (typing || $('layer').childElementCount) return;
  if (!e.ctrlKey && !e.metaKey && !e.altKey && (e.key === '1' || e.key === '2')) {
    call('show_features', { open: e.key === '2' });
    e.preventDefault();
    return;
  }
  if (featureKey(e)) e.preventDefault();
});

document.addEventListener('mousedown', (e) => {
  const layer = $('layer');
  if (layer.querySelector('.menu') && !layer.contains(e.target)) closeLayer();
});

$('tab-workspace').addEventListener('click', () => call('show_features', { open: false }));
$('tab-feature').addEventListener('click', () => call('show_features', { open: true }));

// ---- The screen ----------------------------------------------------------

const screen = (() => {
  const text = $('text');
  const cursorCanvas = $('cursor');
  const terminal = $('terminal');
  const tctx = text.getContext('2d');
  const cctx = cursorCanvas.getContext('2d');

  let pane = null;
  let rows = 0;
  let cols = 0;
  let mirror = [];
  let cursor = { row: 0, col: 0, visible: false, shape: 'block' };
  let cell = { w: 8, h: LINE_HEIGHT, ascent: 10, dpr: 1 };
  let palette = null;
  let fontFamily = 'monospace';

  function measure() {
    fontFamily = getComputedStyle(document.documentElement).getPropertyValue('--font-mono').trim() || 'monospace';
    tctx.font = `${FONT_SIZE}px ${fontFamily}`;
    const m = tctx.measureText('M'.repeat(100));
    const ascent = m.fontBoundingBoxAscent ?? FONT_SIZE * 0.8;
    const descent = m.fontBoundingBoxDescent ?? FONT_SIZE * 0.2;
    cell = {
      w: m.width / 100,
      h: LINE_HEIGHT,
      ascent: (LINE_HEIGHT - (ascent + descent)) / 2 + ascent,
      dpr: window.devicePixelRatio || 1,
    };
  }

  function readPalette() {
    const css = getComputedStyle(document.documentElement);
    const v = (name) => css.getPropertyValue(name).trim();
    palette = { fg: v('--fg'), bg: v('--bg-sunken'), ansi: [] };
    for (let i = 0; i < 16; i++) palette.ansi.push(v(`--ansi-${i}`));
  }

  // Packed as on the wire: 0 the default, 1 << 24 | index, 2 << 24 | rgb.
  function colour(packed, fallback) {
    const kind = packed >>> 24;
    if (kind === 0) return fallback;
    if (kind === 2) return `rgb(${(packed >> 16) & 255},${(packed >> 8) & 255},${packed & 255})`;
    const i = packed & 255;
    if (i < 16) return palette.ansi[i];
    if (i < 232) {
      const n = i - 16;
      const step = (c) => (c ? 55 + 40 * c : 0);
      return `rgb(${step(Math.floor(n / 36))},${step(Math.floor(n / 6) % 6)},${step(n % 6)})`;
    }
    const grey = 8 + 10 * (i - 232);
    return `rgb(${grey},${grey},${grey})`;
  }

  function size() {
    const w = Math.ceil(cols * cell.w);
    const h = rows * cell.h;
    for (const canvas of [text, cursorCanvas]) {
      canvas.width = Math.ceil(w * cell.dpr);
      canvas.height = Math.ceil(h * cell.dpr);
      canvas.style.width = `${w}px`;
      canvas.style.height = `${h}px`;
    }
    tctx.setTransform(cell.dpr, 0, 0, cell.dpr, 0, 0);
    cctx.setTransform(cell.dpr, 0, 0, cell.dpr, 0, 0);
    tctx.textBaseline = 'alphabetic';
  }

  function drawRow(row) {
    const y = row * cell.h;
    tctx.clearRect(0, y, cols * cell.w + 1, cell.h);
    for (const [col, cells, fg, bg, flags] of mirror[row] || []) {
      let fore = colour(fg, palette.fg);
      let back = colour(bg, null);
      if (flags & REVERSE) [fore, back] = [back || palette.bg, fore];
      const x = col * cell.w;
      const width = (flags & WIDE ? 2 : cells.length) * cell.w;
      if (back) {
        tctx.fillStyle = back;
        tctx.fillRect(x, y, width, cell.h);
      }
      tctx.font = `${flags & ITALIC ? 'italic ' : ''}${flags & BOLD ? '700 ' : ''}${FONT_SIZE}px ${fontFamily}`;
      tctx.fillStyle = fore;
      if (flags & WIDE) {
        const glyph = tctx.measureText(cells[0]).width;
        tctx.fillText(cells[0], x + (2 * cell.w - glyph) / 2, y + cell.ascent);
      } else {
        cells.forEach((ch, i) => {
          if (ch !== ' ') tctx.fillText(ch, x + i * cell.w, y + cell.ascent);
        });
      }
      if (flags & UNDERLINE) tctx.fillRect(x, y + cell.h - 3, width, 1);
    }
  }

  function cellAt(row, col) {
    for (const [start, cells, , , flags] of mirror[row] || []) {
      const span = flags & WIDE ? 2 : cells.length;
      if (col >= start && col < start + span) return flags & WIDE ? cells[0] : cells[col - start];
    }
    return ' ';
  }

  function drawCursor() {
    cctx.clearRect(0, 0, cursorCanvas.width, cursorCanvas.height);
    if (!cursor.visible || cursor.row >= rows || cursor.col >= cols) return;
    const x = cursor.col * cell.w;
    const y = cursor.row * cell.h;
    const focused = document.activeElement === $('input') && document.hasFocus();
    cctx.fillStyle = palette.fg;
    cctx.strokeStyle = palette.fg;
    if (cursor.shape === 'bar') {
      cctx.fillRect(x, y, 2, cell.h);
    } else if (cursor.shape === 'underline') {
      cctx.fillRect(x, y + cell.h - 2, cell.w, 2);
    } else if (!focused) {
      cctx.strokeRect(x + 0.5, y + 0.5, cell.w - 1, cell.h - 1);
    } else {
      cctx.fillRect(x, y, cell.w, cell.h);
      cctx.font = `${FONT_SIZE}px ${fontFamily}`;
      cctx.fillStyle = palette.bg;
      cctx.fillText(cellAt(cursor.row, cursor.col), x, y + cell.ascent);
    }
  }

  function repaint() {
    size();
    for (let row = 0; row < rows; row++) drawRow(row);
    drawCursor();
  }

  function frame(f) {
    // A deferred script can run before the stylesheet applies, when every
    // token reads empty; a colour the canvas cannot parse leaves the last
    // one standing, so the palette is read again until it is there.
    if (!palette.fg) readPalette();
    if (f.pane !== pane || f.full) {
      pane = f.pane;
      mirror = new Array(f.rows).fill(null).map(() => []);
    }
    const resized = f.rows !== rows || f.cols !== cols;
    rows = f.rows;
    cols = f.cols;
    if (resized) {
      mirror.length = rows;
      for (let row = 0; row < rows; row++) mirror[row] ||= [];
      size();
      const label = $('size');
      if (label) label.textContent = sizeLabel();
    }
    for (const [row, segments] of f.lines) mirror[row] = segments;
    if (resized) {
      for (let row = 0; row < rows; row++) drawRow(row);
    } else {
      for (const [row] of f.lines) drawRow(row);
    }
    cursor = f.cursor;
    drawCursor();
  }

  function sizeLabel() {
    return cols ? `${cols} × ${rows}` : '';
  }

  // The room the terminal has, in cells. The daemon sizes a pane to the
  // smallest of its viewers, so the grid that comes back may be smaller.
  let sent = null;
  let resizeTimer = null;
  function report() {
    clearTimeout(resizeTimer);
    resizeTimer = setTimeout(() => {
      if (terminal.hidden) return;
      const style = getComputedStyle(terminal);
      const w = terminal.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight);
      const h = terminal.clientHeight - parseFloat(style.paddingTop) - parseFloat(style.paddingBottom);
      const next = { rows: Math.max(1, Math.floor(h / cell.h)), cols: Math.max(1, Math.floor(w / cell.w)) };
      if (sent && sent.rows === next.rows && sent.cols === next.cols) return;
      sent = next;
      call('resize', next);
    }, 50);
  }

  function rescale() {
    measure();
    readPalette();
    repaint();
    sent = null;
    report();
  }

  // A device-pixel-ratio change (a move to another monitor, a zoom) needs
  // a fresh measurement; matchMedia is how a page hears of one.
  function watchScale() {
    const query = matchMedia(`(resolution: ${window.devicePixelRatio}dppx)`);
    query.addEventListener('change', () => {
      rescale();
      watchScale();
    }, { once: true });
  }

  measure();
  readPalette();
  window.addEventListener('load', rescale);
  new ResizeObserver(report).observe(terminal);
  watchScale();
  matchMedia('(prefers-color-scheme: light)').addEventListener('change', () => {
    readPalette();
    repaint();
  });

  return { frame, drawCursor, report, sizeLabel };
})();

// ---- Keys, text and pastes ---------------------------------------------

const input = $('input');
const MODIFIERS = new Set(['Shift', 'Control', 'Alt', 'Meta', 'AltGraph', 'CapsLock']);
const MAC = /Macintosh|Mac OS X/.test(navigator.userAgent);

function isPasteChord(e) {
  const key = e.key.toLowerCase();
  return MAC ? e.metaKey && key === 'v' : e.ctrlKey && e.shiftKey && key === 'v';
}

input.addEventListener('keydown', (e) => {
  // WebKit reports an input method's keys as 229 without isComposing.
  if (e.isComposing || e.keyCode === 229) return;
  // Left to the browser so its paste event fires.
  if (isPasteChord(e)) return;
  // The command key's chords are the window's (quit, close).
  if (MAC && e.metaKey) return;
  if (MODIFIERS.has(e.key)) return;
  // Every other key goes to the pane, so no webview accelerator (F5,
  // Ctrl+F, Ctrl+R) fires behind the person's back.
  e.preventDefault();
  call('key', {
    key: { key: e.key, code: e.code, ctrl: e.ctrlKey, alt: e.altKey, shift: e.shiftKey, meta: e.metaKey },
  });
});

input.addEventListener('compositionend', (e) => {
  if (e.data) call('text', { text: e.data });
  input.value = '';
});

input.addEventListener('input', (e) => {
  if (!e.isComposing) input.value = '';
});

input.addEventListener('paste', (e) => {
  e.preventDefault();
  const text = e.clipboardData?.getData('text/plain');
  if (text) call('paste', { text });
});

input.addEventListener('focus', () => screen.drawCursor());
input.addEventListener('blur', () => screen.drawCursor());
$('terminal').addEventListener('mousedown', (e) => {
  e.preventDefault();
  input.focus();
});

window.addEventListener('focus', () => {
  call('focus', { focused: true });
  screen.drawCursor();
});
window.addEventListener('blur', () => {
  call('focus', { focused: false });
  screen.drawCursor();
});

document.addEventListener('click', (e) => {
  if (state.menuOpen && !$('heading').contains(e.target)) {
    state.menuOpen = false;
    drawHeading();
  }
});

$('reconnect').addEventListener('click', reconnect);

// A pane appearing after the empty stage needs its room reported.
new MutationObserver(() => screen.report()).observe($('terminal'), { attributes: true, attributeFilter: ['hidden'] });

drawChrome();
connect();
