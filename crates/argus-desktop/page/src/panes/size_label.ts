// What the Workspace header says of the grid: where a parked view is, and the
// size only for a moment after it changed, since it is not news at rest.

export interface SizeFacts {
  cols: number;
  rows: number;
  parked: { offset: number; depth: number } | null;
  /** When the size stops showing, in `now`'s clock. */
  shownUntil: number;
  now: number;
}

export function sizeLabel({ cols, rows, parked, shownUntil, now }: SizeFacts): string {
  if (!cols) return '';
  // A parked view leads with how far back it is: otherwise it looks like
  // a pane gone quiet.
  const back = parked ? `↑ ${parked.offset}/${parked.depth}` : '';
  if (now >= shownUntil) return back;
  return back ? `${back} · ${cols} × ${rows}` : `${cols} × ${rows}`;
}
