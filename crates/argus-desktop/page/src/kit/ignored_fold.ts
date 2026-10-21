// Which rows of a file tree fold into the one "N ignored" row at its foot.
// Pure, so the rule is held under node without a page (`test/kit`).

export interface Foldable {
  depth: number;
  ignored: boolean;
  name: string;
}

export interface Folded<T> {
  /** The rows that stay in the tree, in their order. */
  shown: T[];
  /** The rows the fold holds, in their order: an ignored top-level entry and
   *  whatever was opened under it. */
  held: T[];
  /** The ignored top-level entries, which the fold's row names. */
  names: string[];
}

/** Ignored entries at the top of the tree are build output, caches and the
 *  agents' own folders: seven of them between a person and the sources they
 *  came for. Deeper ones stay where they are, since folding one out of the
 *  middle of its folder would lose where it was. */
export function foldIgnored<T extends Foldable>(rows: readonly T[]): Folded<T> {
  const folded: Folded<T> = { shown: [], held: [], names: [] };
  let inside = false;
  for (const row of rows) {
    if (row.depth === 0) {
      inside = row.ignored;
      if (inside) folded.names.push(row.name);
    }
    (inside ? folded.held : folded.shown).push(row);
  }
  return folded;
}

/** What the fold's row says: `4 ignored · .argus, .opencode, target`. */
export function foldLabel(names: readonly string[]): string {
  return `${names.length} ignored · ${names.join(', ')}`;
}
