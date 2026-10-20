// Whether a table cell only repeats its row's status chip.

import type { ExtTable } from '../bindings/ExtTable';
import type { Value } from '../bindings/Value';

/** Case, spacing and brackets differ between a status and its echo:
 *  "exited (137)" repeats "exited 137". */
function plain(text: string): string {
  return text
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, ' ')
    .trim();
}

export function echoes(row: ExtTable['rows'][number], cell: Value | undefined): boolean {
  const word = row.status ? plain(row.status.label) : '';
  if (!word || !cell) return false;
  const text = cell.kind === 'text' ? cell.text : cell.kind === 'chip' ? cell.chip.label : null;
  return text !== null && plain(text) === word;
}
