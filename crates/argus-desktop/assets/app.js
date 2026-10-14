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
  rail: { projects: [], project: null, needsYou: 0, working: 0, shown: null, loaded: false },
  menuOpen: false,
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
  for (const child of children.flat()) {
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
  const shownRepo = state.rail.shown?.repository;
  const rows = [];
  for (const repo of project.repositories) {
    if (repo.divider) rows.push(el('div', { class: 'rule', 'aria-hidden': 'true' }));
    const colour = statusColour(repo.status);
    const dot = colour && el('span', { class: 'rdot' });
    if (dot) dot.style.background = colour;
    rows.push(el('div', { class: `repo${repo.quiet ? ' quiet' : ''}${repo.name === shownRepo ? ' sel' : ''}` },
      el('button', {
        class: 'repobtn',
        'aria-expanded': String(repo.open),
        onclick: () => call('toggle_repository', { repository: repo.id }),
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
  head.hidden = !shown;
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
