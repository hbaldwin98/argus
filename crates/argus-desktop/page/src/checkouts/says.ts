// What a Checkouts row says about itself, as words before they are markup:
// the table's State cell and the branch beside a name. Kept apart from the
// drawing so the rule that a clean, idle checkout says nothing can be held
// under node.

import type { CheckoutsRow } from '../bindings/CheckoutsRow';

/** One thing the State cell says, in the order it is drawn. */
export type Say =
  | { kind: 'detached' }
  | { kind: 'changed'; text: string }
  | { kind: 'distance'; text: string }
  | { kind: 'panes'; count: number }
  | { kind: 'feature'; number: number; viewed: boolean };

/** The State cell of a checkout row. Empty when the checkout is clean, in
 *  step with its upstream and idle: only what needs a look is said, so a
 *  column of quiet rows is a column of good news. */
export function stateOf(row: CheckoutsRow): Say[] {
  const says: Say[] = [];
  if (row.detached) says.push({ kind: 'detached' });
  if (row.changed > 0) says.push({ kind: 'changed', text: `${row.changed} changed` });
  const distance = [row.ahead > 0 && `↑${row.ahead}`, row.behind > 0 && `↓${row.behind}`]
    .filter(Boolean)
    .join(' ');
  if (distance) says.push({ kind: 'distance', text: distance });
  if (row.running.length > 0) says.push({ kind: 'panes', count: row.running.length });
  if (row.feature !== null) says.push({ kind: 'feature', number: row.feature, viewed: row.viewed });
  return says;
}

/** The branch to set beside a checkout's name: only when the folder name
 *  does not already say it. A detached head is the State cell's to say. */
export function branchBeside(row: CheckoutsRow): string | null {
  if (row.name === null || row.detached) return null;
  return row.branch && row.branch !== row.name ? row.branch : null;
}
