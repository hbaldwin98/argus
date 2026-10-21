// The Code stage's editor: a working-tree file open in CodeMirror, written
// as it is typed and changed under the cursor as an agent writes it. The
// buffer is the page's alone — a key is drawn in the handler that heard it
// and never waits on Rust — and writes go to the daemon in the background,
// a burst of typing at a time. Rust says only that the editor is up
// (`CodePage.editing`); `sync` keeps the buffer in step with the disk.
// Loaded the first time a file is edited, so the window does not parse
// CodeMirror at start for a stage it may never open.

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
  StateEffect,
  StateField,
  StreamLanguage,
  Transaction,
  WidgetType,
  bracketMatching,
  closeBrackets,
  closeBracketsKeymap,
  csharp,
  css,
  defaultKeymap,
  drawSelection,
  highlightActiveLine,
  highlightActiveLineGutter,
  highlightSelectionMatches,
  history,
  historyKeymap,
  indentUnit,
  indentWithTab,
  javascript,
  json,
  keymap,
  lineNumbers,
  markdown,
  python,
  rust,
  searchKeymap,
  tagHighlighter,
  tags,
  toml,
  yaml,
  type DecorationSet,
  type Language,
} from '../vendor/codemirror.js';
import { colours } from './colours.js';
import { el } from '../kit/dom.js';
import { call } from '../connection/tauri.js';
import { Synced, type Clash } from './sync.js';

/** How long typing may pause before what was typed is written: short
 *  enough that an agent reads what the person sees, long enough that a
 *  word is one write, not one per letter. */
const QUIET_MS = 200;
/** The longest typing goes unwritten while it never pauses. */
const MOST_MS = 1000;
/** How long an agent's change stays lit. */
const LIT_MS = 1600;
/** How long putting the editor away waits on its last write before the
 *  file is read again without it. */
const LEAVE_MS = 1500;
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
  status: Status;
  said: string;
  quiet: number | null;
  since: number | null;
  /** Put away by Escape: once written, Rust is told the line. */
  leaving: number | null;
  leaveBy: number | null;
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

/** The stylesheet's syntax classes, the ones the reading view's spans use,
 *  for Lezer's tags. */
const syntax = tagHighlighter([
  {
    tag: [
      tags.keyword,
      tags.controlKeyword,
      tags.modifier,
      tags.definitionKeyword,
      tags.moduleKeyword,
      tags.operatorKeyword,
      tags.self,
    ],
    class: 'sx-k',
  },
  { tag: [tags.string, tags.special(tags.string), tags.regexp, tags.character], class: 'sx-s' },
  { tag: [tags.comment, tags.lineComment, tags.blockComment, tags.docComment], class: 'sx-c' },
  { tag: [tags.number, tags.bool, tags.null, tags.atom], class: 'sx-n' },
  { tag: [tags.typeName, tags.className, tags.namespace], class: 'sx-t' },
  {
    tag: [tags.function(tags.variableName), tags.function(tags.propertyName), tags.macroName],
    class: 'sx-f',
  },
  { tag: [tags.propertyName, tags.attributeName, tags.labelName], class: 'sx-p' },
  { tag: [tags.operator, tags.punctuation], class: 'sx-o' },
]);

/** The grammar a file is coloured by, by its extension. Only its parser
 *  is used: `colours` parses outside the key, and indentation copies the
 *  line before rather than asking the grammar. */
function language(path: string): Language | null {
  const ext = path.slice(path.lastIndexOf('.') + 1).toLowerCase();
  switch (ext) {
    case 'rs':
      return rust().language;
    case 'ts':
    case 'mts':
    case 'cts':
      return javascript({ typescript: true }).language;
    case 'tsx':
      return javascript({ typescript: true, jsx: true }).language;
    case 'js':
    case 'mjs':
    case 'cjs':
      return javascript().language;
    case 'jsx':
      return javascript({ jsx: true }).language;
    case 'py':
      return python().language;
    case 'css':
      return css().language;
    case 'json':
      return json().language;
    case 'md':
      return markdown().language;
    case 'yml':
    case 'yaml':
      return yaml().language;
    case 'toml':
      return StreamLanguage.define(toml);
    case 'cs':
      return StreamLanguage.define(csharp);
    default:
      return null;
  }
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
  if (s.status !== 'clash') return [state];
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
    status: 'saved',
    said: '',
    quiet: null,
    since: null,
    leaving: null,
    leaveBy: null,
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
  const grammar = language(s.path);
  const state = EditorState.create({
    doc: text,
    extensions: [
      lineNumbers(),
      highlightActiveLineGutter(),
      history(),
      drawSelection(),
      highlightActiveLine(),
      indentUnit.of('    '),
      bracketMatching(),
      closeBrackets(),
      highlightSelectionMatches(),
      grammar ? colours(grammar, syntax) : [],
      lit,
      clashed,
      s.editable.of(EditorState.readOnly.of(false)),
      keymap.of([
        ...closeBracketsKeymap,
        ...defaultKeymap,
        ...searchKeymap,
        ...historyKeymap,
        indentWithTab,
        // After the defaults, which take Escape first to drop a second
        // cursor; with one cursor left it puts the editor away.
        { key: 'Escape', run: () => (leave(), true) },
        { key: 'Mod-s', run: () => (write(s), true), preventDefault: true },
      ]),
      EditorView.updateListener.of((update) => {
        if (!update.docChanged || update.transactions.some((tr) => tr.annotation(fromDisk))) return;
        s.sync?.typed(update.changes);
        typed(s);
      }),
    ],
  });
  s.view = new EditorView({ state, parent: s.mount });
  goTo(s, s.at);
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

/** Everything written: a leave is told, a retired session closed. */
function done(s: Session): void {
  const sync = s.sync;
  const clean = !sync || (!sync.pending() && !sync.clashing());
  if (s.leaving !== null && (clean || (s.leaveBy !== null && performance.now() > s.leaveBy))) {
    const line = s.leaving;
    s.leaving = null;
    s.leaveBy = null;
    call('code', { gesture: { on: 'edited', line } });
  }
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
  if (s.focus) {
    s.focus = false;
    s.view.focus();
  }
}

/** The editor is no longer up: what is typed is written, and then let go. */
export function retire(s: Session | null = current): void {
  if (!s) return;
  if (current === s) current = null;
  s.retiring = true;
  write(s);
}

/** Escape, or the Done button: the file is read again once written. */
export function leave(): void {
  const s = current;
  if (!s) return;
  if (s.sync?.clashing()) {
    tell(s, 'clash', 'keep yours or take the agent’s first');
    return;
  }
  const view = s.view;
  s.leaving = view ? view.state.doc.lineAt(view.state.selection.main.head).number : s.at;
  s.leaveBy = performance.now() + LEAVE_MS;
  write(s);
  setTimeout(() => done(s), LEAVE_MS + 10);
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
      break;
    }
    case 'saved':
      s.sync?.saved(e.base, e.version);
      write(s);
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
