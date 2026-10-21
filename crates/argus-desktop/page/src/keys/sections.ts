// The shortcuts sheet's sections: the keymap Rust sends, and the extensions'
// headings, dealt into one section per place with the pairs of bindings that
// are one thought written once. Pure, so the sheet stays generated from the
// keymap and node can hold it to listing every binding.

import type { Action } from '../bindings/Action';
import type { Chord } from '../bindings/Chord';

/** What a section is made of: a keymap group or an extension's heading. */
export interface Source {
  /** The place's name, or the extension's title. */
  place: string;
  title: string;
  bindings: readonly { action?: Action; chords: readonly Chord[]; label: string }[];
}

/** How a chord is written, as `chords` says it; handed in so this stays
 *  free of the DOM. */
export interface Namer {
  press(chord: Chord): string;
  leader: string;
}

export interface Row {
  label: string;
  /** The alternatives, each the presses it takes in turn. */
  keys: string[][];
}

export interface Section {
  /** The place's name, the id `sectionFor` answers with. */
  id: string;
  title: string;
  /** The keys that follow the leader, written without it. */
  rows: Row[];
  /** The pane's keys that are not chorded: pasting, paging, the leader sent on. */
  without: Row[];
  /** Whether `rows` follow the leader, which the sheet then says once. */
  chorded: boolean;
}

/** Bindings that are one row. The caps come from the chords themselves, so a
 *  rebound key reads true; a set whose chords disagree stays as it was. */
const MERGES: { actions: Action[]; label: string; arrows: boolean }[] = [
  {
    actions: ['otherSide', 'previousSide'],
    label: 'the keys to the next / previous pane on screen',
    arrows: false,
  },
  {
    actions: ['focusLeft', 'focusRight', 'focusUp', 'focusDown'],
    label: 'the keys to the pane that way',
    arrows: true,
  },
  {
    actions: ['movePaneLeft', 'movePaneRight', 'movePaneUp', 'movePaneDown'],
    label: 'move the pane that way, trading places',
    arrows: true,
  },
];

const ARROW = /\s?([←→↑↓])$/;

/** `← → ↑ ↓`, or `Shift + arrow` when the four hold a modifier; null when
 *  they do not share one. */
function arrowCap(chords: readonly Chord[], namer: Namer): string[] | null {
  const parts = chords.map((chord) => {
    const written = namer.press({ ...chord, leader: false });
    const m = ARROW.exec(written);
    return m ? { prefix: written.slice(0, m.index), glyph: m[1] ?? '' } : null;
  });
  if (parts.some((p) => p === null)) return null;
  const found = parts as { prefix: string; glyph: string }[];
  const prefix = found[0]?.prefix ?? '';
  if (found.some((p) => p.prefix !== prefix)) return null;
  return [prefix ? `${prefix} + arrow` : found.map((p) => p.glyph).join(' ')];
}

function keysOf(chords: readonly Chord[], namer: Namer, bare: boolean): string[][] {
  return chords.map((chord) => {
    const written = namer.press({ ...chord, leader: false });
    return chord.leader && !bare ? [namer.leader, written] : [written];
  });
}

/** The rows of one source: a merged set is one row where its first member
 *  stood. `bare` writes a chorded key without its leader. */
function rowsOf(source: Source, namer: Namer, bare: boolean): Row[] {
  const rows: Row[] = [];
  const done = new Set<number>();
  source.bindings.forEach((binding, i) => {
    if (done.has(i)) return;
    const merge = MERGES.find(
      (m) => binding.action !== undefined && m.actions[0] === binding.action,
    );
    if (merge) {
      const at = merge.actions.map((a) => source.bindings.findIndex((b) => b.action === a));
      if (at.every((j) => j >= 0)) {
        const chords = at.flatMap((j) => source.bindings[j]?.chords ?? []);
        const cap = merge.arrows ? arrowCap(chords, namer) : undefined;
        if (cap !== null) {
          at.forEach((j) => done.add(j));
          rows.push({ label: merge.label, keys: cap ? [cap] : keysOf(chords, namer, bare) });
          return;
        }
      }
    }
    rows.push({ label: binding.label, keys: keysOf(binding.chords, namer, bare) });
  });
  return rows;
}

/** A pane key that follows the leader: all but the leader sent on. */
function keyed(b: Source['bindings'][number]): boolean {
  return b.action !== 'leader' && b.chords.some((c) => c.leader);
}

/** One section per source, in the order given. The pane's chorded keys lose
 *  their leader and the rest wait under "without"; the leader sent on keeps
 *  both its presses, it being the one chord that is the leader twice. */
export function sections(sources: readonly Source[], namer: Namer): Section[] {
  return sources.map((source): Section => {
    const chorded = source.bindings.some((b) => b.chords.some((c) => c.leader));
    if (!chorded) {
      return {
        id: source.place,
        title: source.title,
        rows: rowsOf(source, namer, false),
        without: [],
        chorded,
      };
    }
    const rows = rowsOf({ ...source, bindings: source.bindings.filter(keyed) }, namer, true);
    const without = rowsOf(
      { ...source, bindings: source.bindings.filter((b) => !keyed(b)) },
      namer,
      false,
    );
    return { id: source.place, title: source.title, rows, without, chorded };
  });
}

function text(row: Row): string {
  return `${row.label} ${row.keys.map((k) => k.join(' ')).join(' ')}`.toLowerCase();
}

/** The sections with a row that has every word of `query` in its action or
 *  its key, narrowed to those rows. An empty query is every section. */
export function matching(all: readonly Section[], query: string): Section[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (words.length === 0) return [...all];
  const keep = (rows: Row[]): Row[] =>
    rows.filter((row) => words.every((w) => text(row).includes(w)));
  return all
    .map((s) => ({ ...s, rows: keep(s.rows), without: keep(s.without) }))
    .filter((s) => s.rows.length + s.without.length > 0);
}

/** What the sheet is opened from, as the page knows it. */
export interface Where {
  /** The keys are in a pane's text area. */
  pane: boolean;
  /** A dialog with keys of its own is up. */
  dialog: 'review' | 'history' | 'sessions' | null;
  /** A sidebar row holds the keys. */
  rail: boolean;
  /** A feature is open, and its diagram expanded. */
  outline: boolean;
  diagram: boolean;
  stage: string;
}

/** The section for where the person is: the layer with the keys, as the
 *  window asks them, then the stage. Anywhere when nothing is more exact
 *  or the keymap has no such section. */
export function sectionFor(where: Where, ids: readonly string[]): string {
  const stage: Record<string, string> = {
    feature: 'feature',
    checkouts: 'checkouts',
    code: 'code',
  };
  const wanted = [
    where.dialog,
    where.pane ? 'pane' : null,
    where.rail ? 'rail' : null,
    where.diagram ? 'diagram' : null,
    where.outline ? 'outline' : null,
    stage[where.stage] ?? null,
  ];
  return wanted.find((id): id is string => id !== null && ids.includes(id)) ?? 'anywhere';
}
