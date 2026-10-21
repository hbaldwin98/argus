// The words an open feature's header says, apart from the elements that
// carry them, so a rule about what it says can be held under node.

import type { FeatureView } from '../../bindings/FeatureView';
import type { Link } from '../../bindings/Link';

/** The one button the linked tickets are: a single ticket by name, several
 *  of one tracker by count under its name, several trackers by count alone,
 *  so the header's line stays one length however many are linked. */
export function ticketsLabel(links: readonly Link[]): string | null {
  const [first] = links;
  if (!first) return null;
  if (links.length === 1) return `${first.tracker} ${first.key}`;
  const count = `${links.length} tickets`;
  return links.every((link) => link.tracker === first.tracker)
    ? `${first.tracker} · ${count}`
    : count;
}

/** When the tickets were last reconciled and how much changed since: the
 *  words that sit under Reconcile in the menu rather than on the header. */
export function reconciledSaid(view: Pick<FeatureView, 'reconciled' | 'changes'>): string {
  const changes = view.changes ?? 0;
  const said = [
    view.reconciled
      ? `Reconciled ${view.reconciled.replace(/^reconciled /i, '').replace(/ by .*$/, '')}`
      : null,
    changes > 0 ? `${changes} change${changes === 1 ? '' : 's'} since` : null,
  ].filter((part): part is string => part !== null);
  return said.length ? said.join(' · ') : 'never reconciled';
}

const WAITING: Record<string, string> = {
  asked: 'an agent is reading',
  proposed: 'a proposal waits',
  carrying: 'being carried out',
};

/** The Reconcile entry of the feature's menu. Only a waiting proposal asks
 *  for the person, so only it is `attention`; the other states say where
 *  the reconcile stands. */
export function reconcileEntry(view: Pick<FeatureView, 'reconcile' | 'reconciled' | 'changes'>): {
  label: string;
  hint: string;
  attention: boolean;
} {
  const waiting = view.reconcile ? WAITING[view.reconcile] : undefined;
  return {
    label: waiting ? `Reconcile — ${waiting}` : 'Reconcile with its tickets…',
    hint: reconciledSaid(view),
    attention: view.reconcile === 'proposed',
  };
}
