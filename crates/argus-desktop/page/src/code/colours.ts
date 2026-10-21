// The editor's syntax colours, parsed and drawn after a key rather than in
// it. CodeMirror's own language support parses inside the transaction a
// key makes and redraws every visible line's colours with it: over a
// millisecond of each key on a small file, most of what a key cost. Here
// the colours already drawn move with the text at once, and the parse runs
// in a task of its own straight after the key, which lands before the frame
// that paints it; a long parse is cut into slices, so a key never waits.

import {
  Decoration,
  RangeSetBuilder,
  TreeFragment,
  ViewPlugin,
  highlightTree,
  type DecorationSet,
  type EditorView,
  type Extension,
  type Highlighter,
  type Input,
  type Language,
  type PartialParse,
  type Text,
  type ViewUpdate,
} from '../vendor/codemirror.js';

/** How long one slice of a parse may run before it lets a key in. */
const SLICE_MS = 6;

/** Tasks run after the current one, ahead of any timer: a parse queued by
 *  a key is done before the frame that draws the key, as a rule. */
const channel = new MessageChannel();
const tasks: (() => void)[] = [];
channel.port1.addEventListener('message', () => tasks.shift()?.());
channel.port1.start();
function soon(task: () => void): void {
  tasks.push(task);
  channel.port2.postMessage(null);
}

/** A document read by the parser a line at a time, never copied whole. */
class TextInput implements Input {
  readonly lineChunks = true;
  readonly length: number;
  constructor(readonly doc: Text) {
    this.length = doc.length;
  }
  chunk(from: number): string {
    const line = this.doc.lineAt(from);
    const rest = line.text.slice(from - line.from);
    return line.to < this.doc.length ? `${rest}\n` : rest;
  }
  read(from: number, to: number): string {
    return this.doc.sliceString(from, to);
  }
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

export function colours(language: Language, highlighter: Highlighter): Extension {
  const parser = language.parser;
  return ViewPlugin.fromClass(
    class {
      decorations: DecorationSet = Decoration.none;
      private fragments: readonly TreeFragment[] = [];
      private parse: PartialParse | null = null;
      private tree: ReturnType<PartialParse['advance']> = null;
      private queued = false;
      private gone = false;

      constructor(readonly view: EditorView) {
        this.queue();
      }

      update(u: ViewUpdate): void {
        if (u.docChanged) {
          // Drawn where the text moved to; what was typed is coloured by
          // the parse that follows.
          this.decorations = this.decorations.map(u.changes);
          const changed: { fromA: number; toA: number; fromB: number; toB: number }[] = [];
          u.changes.iterChangedRanges((fromA, toA, fromB, toB) =>
            changed.push({ fromA, toA, fromB, toB }),
          );
          this.fragments = TreeFragment.applyChanges(this.fragments, changed);
          this.parse = null;
          this.tree = null;
          this.queue();
        } else if (u.viewportChanged && this.tree) {
          this.decorations = this.draw();
        }
      }

      destroy(): void {
        this.gone = true;
      }

      private queue(): void {
        if (this.queued) return;
        this.queued = true;
        soon(() => this.work());
      }

      private work(): void {
        this.queued = false;
        if (this.gone) return;
        this.parse ??= parser.startParse(new TextInput(this.view.state.doc), this.fragments);
        const until = performance.now() + SLICE_MS;
        let tree = this.parse.advance();
        while (!tree) {
          if (performance.now() > until) {
            this.queue();
            return;
          }
          tree = this.parse.advance();
        }
        this.parse = null;
        this.tree = tree;
        this.fragments = TreeFragment.addTree(tree, this.fragments);
        this.decorations = this.draw();
        // A transaction of nothing, for the view to read the colours.
        this.view.dispatch({});
      }

      private draw(): DecorationSet {
        const tree = this.tree;
        if (!tree) return this.decorations;
        const out = new RangeSetBuilder<Decoration>();
        for (const { from, to } of this.view.visibleRanges)
          highlightTree(
            tree,
            highlighter,
            (start, end, cls) => out.add(start, end, mark(cls)),
            from,
            to,
          );
        return out.finish();
      }
    },
    { decorations: (plugin) => plugin.decorations },
  );
}
