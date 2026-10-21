// The editor's syntax colours, from a tree-sitter grammar, parsed and drawn
// after a key rather than in it. The colours already drawn move with the
// text at once; the tree is told what changed and parsed again in a task of
// its own straight after the key, which lands before the frame that paints
// it, so a key costs only its text. Only the lines drawn are coloured.

import {
  Decoration,
  Edit,
  Parser,
  RangeSetBuilder,
  ViewPlugin,
  type ChangeSet,
  type DecorationSet,
  type EditorView,
  type Extension,
  type Point,
  type Text,
  type TSTree,
  type ViewUpdate,
} from '../vendor/codemirror.js';
import type { Loaded } from './grammar/load.js';

/** How large a file is parsed before its first paint rather than after:
 *  one opened coloured, not plain for a frame, at a cost paid once. */
const PARSE_FIRST = 256 * 1024;

/** A capture's colour by its first word, as the daemon colours the read
 *  view (`CAPTURES` in argusd's `syntax/highlight`), so both agree: a
 *  `keyword.control` is a keyword. Plain variables are left uncoloured. */
const CLASSES: Record<string, string> = {
  attribute: 'sx-p',
  boolean: 'sx-n',
  comment: 'sx-c',
  constant: 'sx-n',
  constructor: 'sx-t',
  escape: 'sx-s',
  function: 'sx-f',
  keyword: 'sx-k',
  label: 'sx-n',
  module: 'sx-t',
  namespace: 'sx-t',
  number: 'sx-n',
  operator: 'sx-o',
  property: 'sx-p',
  punctuation: 'sx-o',
  string: 'sx-s',
  tag: 'sx-t',
  type: 'sx-t',
};

export function classOf(capture: string): string | null {
  const dot = capture.indexOf('.');
  return CLASSES[dot < 0 ? capture : capture.slice(0, dot)] ?? null;
}

/** Tasks run after the current one, ahead of any timer: a parse queued by
 *  a key is done before the frame that draws the key, as a rule. */
let channel: MessageChannel | null = null;
const tasks: (() => void)[] = [];
function soon(task: () => void): void {
  // Opened on first use: an open port keeps a process that only imports
  // this module, a test, from ever exiting.
  if (!channel) {
    channel = new MessageChannel();
    channel.port1.addEventListener('message', () => tasks.shift()?.());
    channel.port1.start();
  }
  tasks.push(task);
  channel.port2.postMessage(null);
}

function pointAt(doc: Text, pos: number): Point {
  const line = doc.lineAt(pos);
  return { row: line.number - 1, column: pos - line.from };
}

/** Where `text` ends when it starts at `from`. */
function pointAfter(from: Point, text: string): Point {
  const last = text.lastIndexOf('\n');
  if (last < 0) return { row: from.row, column: from.column + text.length };
  let rows = 0;
  for (let i = text.indexOf('\n'); i >= 0; i = text.indexOf('\n', i + 1)) rows++;
  return { row: from.row + rows, column: text.length - last - 1 };
}

/** The tree-sitter edits a change set makes, each positioned in the text
 *  as the edits before it left it, the order a tree takes them in. */
export function edits(changes: ChangeSet, before: Text, after: Text): Edit[] {
  const out: Edit[] = [];
  changes.iterChanges((fromA, toA, fromB, toB, inserted) => {
    const startPosition = pointAt(after, fromB);
    out.push(
      new Edit({
        startIndex: fromB,
        oldEndIndex: fromB + (toA - fromA),
        newEndIndex: toB,
        startPosition,
        oldEndPosition: pointAfter(startPosition, before.sliceString(fromA, toA)),
        newEndPosition: pointAfter(startPosition, inserted.toString()),
      }),
    );
  });
  return out;
}

/** The document a chunk at a time, never copied whole. */
export function reader(doc: Text): (index: number) => string | undefined {
  return (index) => (index >= doc.length ? undefined : doc.sliceString(index, index + 4096));
}

const marks = new Map<string, Decoration>();
function mark(cls: string): Decoration {
  let found = marks.get(cls);
  if (!found) {
    found = Decoration.mark({ class: cls });
    marks.set(cls, found);
  }
  return found;
}

export function colours(grammar: Loaded): Extension {
  return ViewPlugin.fromClass(
    class {
      decorations: DecorationSet = Decoration.none;
      private parser = new Parser();
      private tree: TSTree | null = null;
      /** The tree has been told of every change since it was parsed. */
      private stale = true;
      private queued = false;
      private gone = false;

      constructor(readonly view: EditorView) {
        this.parser.setLanguage(grammar.language);
        if (view.state.doc.length <= PARSE_FIRST) this.parse();
        else this.queue();
      }

      update(u: ViewUpdate): void {
        if (u.docChanged) {
          this.decorations = this.decorations.map(u.changes);
          if (this.tree)
            for (const e of edits(u.changes, u.startState.doc, u.state.doc)) this.tree.edit(e);
          this.stale = true;
          this.queue();
        } else if (u.viewportChanged && !this.stale) {
          this.decorations = this.draw();
        }
      }

      destroy(): void {
        this.gone = true;
        this.tree?.delete();
        this.parser.delete();
      }

      private queue(): void {
        if (this.queued) return;
        this.queued = true;
        soon(() => this.work());
      }

      private work(): void {
        this.queued = false;
        if (this.gone || !this.stale || !this.parse()) return;
        // A transaction of nothing, for the view to read the colours.
        this.view.dispatch({});
      }

      /** Parses and colours, saying whether it could. */
      private parse(): boolean {
        const next = this.parser.parse(reader(this.view.state.doc), this.tree);
        if (!next) return false;
        this.tree?.delete();
        this.tree = next;
        this.stale = false;
        this.decorations = this.draw();
        return true;
      }

      private draw(): DecorationSet {
        const tree = this.tree;
        if (!tree) return this.decorations;
        const out = new RangeSetBuilder<Decoration>();
        let end = 0;
        const doc = this.view.state.doc;
        // The lines drawn, a margin past the window included, so a short
        // scroll shows no line yet to be coloured.
        for (const { from, to } of [this.view.viewport]) {
          // By rows: the runtime reads a query's indexes as bytes, two to
          // each of the UTF-16 units its nodes are counted in.
          for (const { name, node } of grammar.query.captures(tree.rootNode, {
            startPosition: { row: doc.lineAt(from).number - 1, column: 0 },
            endPosition: { row: doc.lineAt(to).number, column: 0 },
          })) {
            const cls = classOf(name);
            // The first capture of a run wins; one inside it is skipped.
            if (!cls || node.startIndex < end || node.endIndex <= node.startIndex) continue;
            out.add(node.startIndex, node.endIndex, mark(cls));
            end = node.endIndex;
          }
        }
        return out.finish();
      }
    },
    { decorations: (plugin) => plugin.decorations },
  );
}
