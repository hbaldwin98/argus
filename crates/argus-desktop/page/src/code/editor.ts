// The Code stage's editor: a working-tree file open in CodeMirror, written
// as it is typed and changed under the cursor as an agent writes it. The
// buffer is the page's alone — a key is drawn in the handler that heard it
// and never waits on Rust — and writes go to the daemon in the background,
// a burst of typing at a time. Rust says only that the editor is up
// (`CodePage.editing`, every working-tree file it reads); `sync` keeps the
// buffer in step with the disk. Vim's keys are on unless Settings turns
// them off. Loaded the first time a file is shown, so the window does not
// parse CodeMirror at start for a stage it may never open.

import type { CodePage } from '../bindings/CodePage';
import type { EditEvent } from '../bindings/EditEvent';
import type { EditGesture } from '../bindings/EditGesture';
import {
  Annotation,
  ChangeSet,
  Compartment,
  Decoration,
  EditorSelection,
  EditorState,
  EditorView,
  GutterMarker,
  RangeSet,
  StateEffect,
  StateField,
  Transaction,
  Vim,
  WidgetType,
  bracketMatching,
  closeBrackets,
  closeBracketsKeymap,
  defaultKeymap,
  drawSelection,
  gutter,
  highlightActiveLine,
  highlightActiveLineGutter,
  highlightSelectionMatches,
  history,
  historyKeymap,
  indentUnit,
  indentWithTab,
  keymap,
  lineNumbers,
  searchKeymap,
  vim,
  type DecorationSet,
} from '../vendor/codemirror.js';
import { colours } from './colours.js';
import { offerFor, type Offer } from './grammar/fetch.js';
import { install, installedFor, type Loaded } from './grammar/load.js';
import { el } from '../kit/dom.js';
import { call } from '../connection/tauri.js';
import { state as page } from '../state.js';
import { Synced, type Clash } from './sync.js';

/** How long typing may pause before what was typed is written: short
 *  enough that an agent reads what the person sees, long enough that a
 *  word is one write, not one per letter. */
const QUIET_MS = 200;
/** The longest typing goes unwritten while it never pauses. */
const MOST_MS = 1000;
/** How long an agent's change stays lit. */
const LIT_MS = 1600;
/** How long after a write settles the file is read again for its change
 *  marks and outline: a burst of writes is read once. */
const REREAD_MS = 400;
/** How long after a failed write it is tried again. */
const RETRY_MS = 1000;

type Status =
  | 'opening'
  | 'saved'
  | 'unsaved'
  | 'saving'
  | 'clash'
  | 'failed'
  | 'closed'
  | 'offline';

interface Session {
  key: string;
  checkout: number;
  path: string;
  el: HTMLElement;
  bar: HTMLElement;
  mount: HTMLElement;
  view: EditorView | null;
  sync: Synced | null;
  editable: Compartment;
  keys: Compartment;
  colour: Compartment;
  /** A grammar to install, waiting on the person's word. */
  offer: Offer | null;
  /** What the grammar bar says while one installs or after it failed. */
  grammar: string;
  status: Status;
  said: string;
  quiet: number | null;
  since: number | null;
  reread: number | null;
  /** The change marks last drawn, as Rust read them. */
  marked: string;
  /** The page they came on, kept until the editor mounts. */
  page: CodePage | null;
  /** No longer the one shown: closed once everything is written. */
  retiring: boolean;
  /** Opened again after the connection came back. */
  reopen: boolean;
  /** The line Rust last asked for, from one. */
  at: number;
  focus: boolean;
}

const sessions = new Map<string, Session>();
let current: Session | null = null;
const byView = new WeakMap<EditorView, Session>();
/** Grammars the person said not now to, until the window closes. */
const declined = new Set<string>();

function vimOn(): boolean {
  return page.settings?.chosen.vim ?? true;
}

function autoInstall(): boolean {
  return page.settings?.chosen.installGrammars ?? false;
}

const keyOf = (checkout: number, path: string) => `${checkout}\n${path}`;

function ask(gesture: EditGesture): void {
  call('edit', { gesture });
}

// What the disk did, which is neither typed nor undone.
const fromDisk = Annotation.define<boolean>();

const litMark = Decoration.mark({ class: 'cm-agent-edit' });
const light = StateEffect.define<{ from: number; to: number }[]>();
const unlight = StateEffect.define<null>();
const lit = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(marks, tr) {
    marks = marks.map(tr.changes);
    for (const e of tr.effects) {
      if (e.is(light))
        marks = marks.update({
          add: e.value.filter((r) => r.to > r.from).map((r) => litMark.range(r.from, r.to)),
          sort: true,
        });
      else if (e.is(unlight)) marks = Decoration.none;
    }
    return marks;
  },
  provide: (field) => EditorView.decorations.from(field),
});

class Theirs extends WidgetType {
  constructor(readonly text: string) {
    super();
  }
  override eq(other: Theirs): boolean {
    return other.text === this.text;
  }
  toDOM(): HTMLElement {
    return el(
      'span',
      { class: 'cm-theirs', title: 'What the agent wrote here' },
      this.text === '' ? 'removed' : this.text,
    );
  }
}

const clashMark = Decoration.mark({ class: 'cm-clash' });
const showClashes = StateEffect.define<Clash[]>();
const clashed = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(marks, tr) {
    marks = marks.map(tr.changes);
    for (const e of tr.effects) {
      if (!e.is(showClashes)) continue;
      const ranges = [];
      for (const c of e.value) {
        if (c.to > c.from) ranges.push(clashMark.range(c.from, c.to));
        ranges.push(Decoration.widget({ widget: new Theirs(c.theirs), side: 1 }).range(c.to));
      }
      marks = Decoration.set(ranges, true);
    }
    return marks;
  },
  provide: (field) => EditorView.decorations.from(field),
});

class ChangeMark extends GutterMarker {
  constructor(readonly kind: 'added' | 'removed') {
    super();
  }
  override eq(other: ChangeMark): boolean {
    return other.kind === this.kind;
  }
  override toDOM(): HTMLElement {
    return el('span', { class: `cm-change ${this.kind}` });
  }
}
const added = new ChangeMark('added');
const removed = new ChangeMark('removed');

/** Which lines this version changed, as Rust last read the file, moved
 *  with what is typed until it is read again. */
const markChanges = StateEffect.define<RangeSet<GutterMarker>>();
const changed = StateField.define<RangeSet<GutterMarker>>({
  create: () => RangeSet.empty,
  update(marks, tr) {
    marks = marks.map(tr.changes);
    for (const e of tr.effects) if (e.is(markChanges)) marks = e.value;
    return marks;
  },
});

function changeMarks(view: EditorView, c: CodePage): RangeSet<GutterMarker> {
  const doc = view.state.doc;
  const out = [];
  for (const [i, row] of c.lines.entries()) {
    if (i + 1 > doc.lines) break;
    const at = doc.line(i + 1).from;
    if (row.added) out.push(added.range(at));
    else if (row.removedBefore > 0) out.push(removed.range(at));
  }
  return RangeSet.of(out, true);
}

/** Draws the change marks of the page Rust sent, when they are new. */
function mark(s: Session, c: CodePage): void {
  s.page = c;
  const view = s.view;
  if (!view) return;
  const rows = c.lines.map((r) => (r.added ? '+' : r.removedBefore > 0 ? '-' : '.')).join('');
  const key = `${c.lines.length}:${rows}`;
  if (key === s.marked) return;
  s.marked = key;
  view.dispatch({ effects: markChanges.of(changeMarks(view, c)) });
}

/** The next change after the cursor's line, or the last before it. A run
 *  of changed lines is one change, stepped to at its first line. */
function stepChange(view: EditorView, forward: boolean): boolean {
  const doc = view.state.doc;
  const here = doc.lineAt(view.state.selection.main.head).number;
  let target: number | null = null;
  let previous = -2;
  view.state.field(changed).between(0, doc.length, (from) => {
    const line = doc.lineAt(from).number;
    const run = line === previous + 1;
    previous = line;
    if (run) return;
    if (forward && line > here && target === null) target = from;
    if (!forward && line < here) target = from;
  });
  if (target === null) return false;
  view.dispatch({
    selection: EditorSelection.cursor(target),
    effects: EditorView.scrollIntoView(target, { y: 'center' }),
  });
  return true;
}

// Vim's own ways to write and to step between changes.
Vim.defineEx('write', 'w', (cm) => {
  const s = byView.get(cm.cm6);
  if (s) write(s);
});
Vim.defineAction('nextChange', (cm) => void stepChange(cm.cm6, true));
Vim.defineAction('previousChange', (cm) => void stepChange(cm.cm6, false));
Vim.mapCommand(']c', 'action', 'nextChange', {}, { context: 'normal' });
Vim.mapCommand('[c', 'action', 'previousChange', {}, { context: 'normal' });

/** The lines the selection covers become the stage's pick, and the
 *  comment box opens on them, as `v` and `c` do in a review. */
function comment(view: EditorView): boolean {
  const { from, to } = view.state.selection.main;
  const first = view.state.doc.lineAt(from).number;
  const last = view.state.doc.lineAt(to).number;
  call('code', { gesture: { on: 'click', target: { on: 'range', from: first, to: last } } });
  call('code', { gesture: { on: 'action', action: 'comment' } });
  return true;
}

const SAID: Record<Status, string> = {
  opening: 'Opening…',
  saved: 'Saved',
  unsaved: 'Unsaved',
  saving: 'Saving…',
  clash: 'An agent changed what you were editing',
  failed: 'Not saved',
  closed: 'Closed',
  offline: 'Disconnected; kept here until the daemon is back',
};

/** Draws the bar only when what it says changed: it is set on every key. */
function tell(s: Session, status: Status, said = ''): void {
  if (s.status === status && s.said === said) return;
  s.status = status;
  s.said = said;
  s.bar.replaceChildren(...bar(s));
}

function bar(s: Session): HTMLElement[] {
  const state = el(
    'span',
    { class: `ed-state ${s.status}` },
    s.said ? `${SAID[s.status]}: ${s.said}` : SAID[s.status],
  );
  if (s.status !== 'clash') return [state, ...grammarBar(s)];
  return [
    state,
    el('span', { class: 'grow' }),
    el('button', { class: 'btn sm', onclick: () => resolve(s, 'mine') }, 'Keep mine'),
    el(
      'button',
      { class: 'btn sm primary', onclick: () => resolve(s, 'theirs') },
      "Take the agent's",
    ),
  ];
}

function size(bytes: number): string {
  return bytes >= 1024 * 1024
    ? `${(bytes / 1024 / 1024).toFixed(1)} MB`
    : `${Math.ceil(bytes / 1024)} KB`;
}

/** The grammar offered for this file, or what became of installing it. */
function grammarBar(s: Session): HTMLElement[] {
  if (s.grammar)
    return [el('span', { class: 'grow' }), el('span', { class: 'ed-grammar' }, s.grammar)];
  const offer = s.offer;
  if (!offer) return [];
  const bytes = [offer.wasm, ...offer.queries].reduce((sum, f) => sum + f.size, 0);
  const from = offer.pinned
    ? `${offer.source} from npm, ${size(bytes)}`
    : `${offer.source} from npm${offer.publisher ? ` by ${offer.publisher}` : ''}, ${size(bytes)}; found by searching, not checked by Argus`;
  return [
    el('span', { class: 'grow' }),
    el(
      'span',
      { class: 'ed-grammar' },
      `No colours for ${offer.label} yet. Install its tree-sitter grammar? `,
      el('span', { class: 'mono' }, from),
    ),
    el('button', { class: 'btn sm primary', onclick: () => void installOffer(s) }, 'Install'),
    el('button', { class: 'btn sm', onclick: () => notNow(s) }, 'Not now'),
  ];
}

function redrawBar(s: Session): void {
  s.bar.replaceChildren(...bar(s));
}

/** Colours every open editor of the grammar's files. */
function colourWith(grammar: Loaded, path: string): void {
  for (const other of sessions.values()) {
    if (!other.view || !sameKind(other.path, path)) continue;
    other.offer = null;
    other.grammar = '';
    other.view.dispatch({ effects: other.colour.reconfigure(colours(grammar)) });
    redrawBar(other);
  }
}

/** A file's extension, or its whole name when it has none. */
const extOf = (p: string) =>
  p
    .slice(p.lastIndexOf('/') + 1)
    .split('.')
    .slice(1)
    .pop() ?? p;

function sameKind(a: string, b: string): boolean {
  return extOf(a).toLowerCase() === extOf(b).toLowerCase();
}

/** The file's grammar if it is installed; otherwise what to offer, or the
 *  install itself where Settings says not to ask for one Argus pinned. */
async function findGrammar(s: Session): Promise<void> {
  try {
    const got = await installedFor(s.path);
    if (got) {
      colourWith(got, s.path);
      return;
    }
  } catch (e) {
    s.grammar = `Could not load the grammar: ${String(e)}`;
    redrawBar(s);
    return;
  }
  const offer = await offerFor(s.path);
  if (!offer || declined.has(offer.name)) return;
  s.offer = offer;
  if (offer.pinned && autoInstall()) {
    await installOffer(s);
    return;
  }
  redrawBar(s);
}

async function installOffer(s: Session): Promise<void> {
  const offer = s.offer;
  if (!offer) return;
  s.offer = null;
  s.grammar = `Installing the ${offer.label} grammar…`;
  redrawBar(s);
  try {
    colourWith(await install(offer, s.path), s.path);
  } catch (e) {
    s.grammar = `Could not install ${offer.source}: ${e instanceof Error ? e.message : String(e)}`;
    redrawBar(s);
  }
}

function notNow(s: Session): void {
  if (s.offer) declined.add(s.offer.name);
  s.offer = null;
  redrawBar(s);
  s.view?.focus();
}

function settled(s: Session): Status {
  if (!s.sync) return 'opening';
  if (s.sync.clashing()) return 'clash';
  return s.sync.pending() ? 'saving' : 'saved';
}

function create(checkout: number, path: string, at: number): Session {
  const status = el('div', { class: 'ed-bar' });
  const mount = el('div', { class: 'ed-mount' });
  const s: Session = {
    key: keyOf(checkout, path),
    checkout,
    path,
    el: el('div', { class: 'code-editor' }, status, mount),
    bar: status,
    mount,
    view: null,
    sync: null,
    editable: new Compartment(),
    keys: new Compartment(),
    colour: new Compartment(),
    offer: null,
    grammar: '',
    status: 'saved',
    said: '',
    quiet: null,
    since: null,
    reread: null,
    marked: '',
    page: null,
    retiring: false,
    reopen: false,
    at,
    focus: true,
  };
  tell(s, 'opening');
  sessions.set(s.key, s);
  ask({ on: 'open', checkout, path });
  return s;
}

function mountView(s: Session, text: string): void {
  const sync = new Synced(text, 0);
  s.sync = sync;
  const state = EditorState.create({
    doc: text,
    extensions: [
      // First: Vim's keys are heard before any other keymap's.
      s.keys.of(vimOn() ? vim() : []),
      lineNumbers(),
      gutter({ class: 'cm-changes', markers: (v) => v.state.field(changed) }),
      changed,
      highlightActiveLineGutter(),
      history(),
      drawSelection(),
      highlightActiveLine(),
      indentUnit.of('    '),
      bracketMatching(),
      closeBrackets(),
      highlightSelectionMatches(),
      // Filled once the file's grammar is found installed, or installed.
      s.colour.of([]),
      lit,
      clashed,
      s.editable.of(EditorState.readOnly.of(false)),
      keymap.of([
        ...closeBracketsKeymap,
        ...defaultKeymap,
        ...searchKeymap,
        ...historyKeymap,
        indentWithTab,
        { key: 'Mod-s', run: () => (write(s), true), preventDefault: true },
        // The stage's own keys that must work with the editor holding them.
        { key: 'Mod-p', run: () => (stage('filter'), true), preventDefault: true },
        { key: 'Mod-Shift-f', run: () => (stage('search'), true), preventDefault: true },
        { key: 'Alt-c', run: comment, preventDefault: true },
        { key: 'Alt-]', run: (v) => stepChange(v, true) },
        { key: 'Alt-[', run: (v) => stepChange(v, false) },
      ]),
      EditorView.updateListener.of((update) => {
        if (!update.docChanged || update.transactions.some((tr) => tr.annotation(fromDisk))) return;
        s.sync?.typed(update.changes);
        typed(s);
      }),
    ],
  });
  s.view = new EditorView({ state, parent: s.mount });
  byView.set(s.view, s);
  if (s.page) mark(s, s.page);
  goTo(s, s.at);
  void findGrammar(s);
}

function stage(action: 'filter' | 'search'): void {
  call('code', { gesture: { on: 'action', action } });
}

/** The file is read again once writes stop, for its change marks and
 *  outline; only the one on screen is. */
function readAgain(s: Session): void {
  if (s.reread !== null) clearTimeout(s.reread);
  s.reread = setTimeout(() => {
    s.reread = null;
    if (current === s) call('code', { gesture: { on: 'written' } });
  }, REREAD_MS);
}

function goTo(s: Session, line: number): void {
  const view = s.view;
  if (!view) return;
  const at = view.state.doc.line(Math.max(1, Math.min(line, view.state.doc.lines))).from;
  view.dispatch({
    selection: EditorSelection.cursor(at),
    effects: EditorView.scrollIntoView(at, { y: 'center' }),
  });
}

/** Something was typed: written once typing pauses, or at the latest
 *  `MOST_MS` after it began. */
function typed(s: Session): void {
  const now = performance.now();
  s.since ??= now;
  if (s.quiet !== null) clearTimeout(s.quiet);
  if (now - s.since >= MOST_MS) {
    write(s);
    return;
  }
  s.quiet = setTimeout(() => write(s), QUIET_MS);
  if (s.status === 'saved') tell(s, 'unsaved');
}

/** Writes what is typed, unless a write is in flight or a clash held:
 *  either one's answer writes again. */
function write(s: Session): void {
  if (s.quiet !== null) clearTimeout(s.quiet);
  s.quiet = null;
  s.since = null;
  const sync = s.sync;
  // A file closed takes nothing more; what is typed stays on screen.
  if (!sync || s.status === 'closed') {
    done(s);
    return;
  }
  const base = sync.version;
  const changes = sync.take();
  if (changes) ask({ on: 'change', checkout: s.checkout, path: s.path, base, changes });
  tell(s, settled(s));
  done(s);
}

/** Everything written: a retired session is closed. */
function done(s: Session): void {
  const sync = s.sync;
  const clean = !sync || (!sync.pending() && !sync.clashing());
  if (s.retiring && (clean || s.status === 'closed')) {
    ask({ on: 'close', checkout: s.checkout, path: s.path });
    s.view?.destroy();
    s.el.remove();
    sessions.delete(s.key);
    if (current === s) current = null;
  }
}

/** The disk's change, shown without being typed or undoable, and lit. */
function fromTheDisk(s: Session, change: ChangeSet | null): void {
  const view = s.view;
  const sync = s.sync;
  if (!view || !sync) return;
  const ranges: { from: number; to: number }[] = [];
  change?.iterChangedRanges((_fa, _ta, from, to) => ranges.push({ from, to }));
  const effects: StateEffect<unknown>[] = [showClashes.of(sync.clashes())];
  if (ranges.length > 0) effects.push(light.of(ranges));
  view.dispatch({
    ...(change && !change.empty ? { changes: change } : {}),
    effects,
    annotations: [fromDisk.of(true), Transaction.addToHistory.of(false)],
  });
  if (ranges.length > 0) setTimeout(() => s.view?.dispatch({ effects: unlight.of(null) }), LIT_MS);
}

function resolve(s: Session, keep: 'mine' | 'theirs'): void {
  if (!s.sync) return;
  fromTheDisk(s, s.sync.resolve(keep));
  write(s);
  s.view?.focus();
}

/** The editor for the file `c` reads, kept across redraws: the stage
 *  builds itself anew each time, and the buffer must not go with it. */
export function host(c: CodePage): HTMLElement {
  const path = c.path ?? '';
  const key = keyOf(c.checkout, path);
  if (current && current.key !== key) retire(current);
  let s = sessions.get(key);
  if (!s) s = create(c.checkout, path, c.at);
  else if (s.reopen) {
    s.reopen = false;
    tell(s, 'opening');
    ask({ on: 'open', checkout: s.checkout, path: s.path });
  }
  s.retiring = false;
  if (s !== current) s.focus = true;
  current = s;
  mark(s, c);
  // A line Rust moved to — the outline, a change — is gone to; the
  // editor's own cursor moves are never told it.
  if (c.at !== s.at) {
    s.at = c.at;
    goTo(s, c.at);
  }
  return s.el;
}

/** Once the stage is drawn: an editor just shown takes the keys. */
export function shown(): void {
  const s = current;
  if (!s?.view) return;
  s.view.requestMeasure();
  // Just shown, or the keys let go by a box that closed: back to the file.
  if (s.focus || document.activeElement === document.body) {
    s.focus = false;
    s.view.focus();
  }
}

/** Settings changed: Vim's keys on or off in every editor open. */
export function prefs(): void {
  const on = vimOn();
  for (const s of sessions.values())
    s.view?.dispatch({ effects: s.keys.reconfigure(on ? vim() : []) });
}

/** The editor is no longer up: what is typed is written, and then let go. */
export function retire(s: Session | null = current): void {
  if (!s) return;
  if (current === s) current = null;
  s.retiring = true;
  write(s);
}

export function receive(e: EditEvent): void {
  if (e.on === 'lost') {
    for (const s of sessions.values()) {
      s.sync?.lost();
      s.reopen = true;
      tell(s, 'offline');
    }
    return;
  }
  const s = sessions.get(keyOf(e.checkout, e.path));
  if (!s) return;
  switch (e.on) {
    case 'opened': {
      if (!s.sync) mountView(s, e.text);
      else fromTheDisk(s, s.sync.reopened(e.text, e.version));
      s.view?.dispatch({ effects: s.editable.reconfigure(EditorState.readOnly.of(false)) });
      if (s.view && current === s) shown();
      write(s);
      break;
    }
    case 'changed': {
      const sync = s.sync;
      if (!sync) return;
      const shownChange = sync.changed(e.from, e.version, e.changes);
      if (shownChange === null) {
        ask({ on: 'open', checkout: s.checkout, path: s.path });
        return;
      }
      fromTheDisk(s, shownChange);
      write(s);
      readAgain(s);
      break;
    }
    case 'saved':
      s.sync?.saved(e.base, e.version);
      write(s);
      readAgain(s);
      break;
    case 'refused':
      s.sync?.refused(e.base, e.reason);
      if (e.reason === null) return;
      tell(s, 'failed', e.reason);
      setTimeout(() => write(s), RETRY_MS);
      break;
    case 'closed':
      tell(s, 'closed', e.reason);
      s.view?.dispatch({ effects: s.editable.reconfigure(EditorState.readOnly.of(true)) });
      done(s);
      break;
  }
}
