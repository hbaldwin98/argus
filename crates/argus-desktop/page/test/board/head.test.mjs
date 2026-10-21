// `board/lenses/head_words` under node: what the open feature's header and
// its menu say about the linked tickets and the reconcile.

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { reconcileEntry, ticketsLabel } from '../../src/board/lenses/head_words.ts';

const link = (tracker, key) => ({ tracker, key, url: `https://x/${key}`, role: '' });

test('the linked tickets are one button whatever their number', () => {
  assert.equal(ticketsLabel([]), null);
  assert.equal(ticketsLabel([link('GitHub', 'orion#212')]), 'GitHub orion#212');
  assert.equal(
    ticketsLabel([link('GitHub', 'orion#212'), link('GitHub', 'orion#9')]),
    'GitHub · 2 tickets',
  );
  assert.equal(ticketsLabel([link('GitHub', 'orion#212'), link('Jira', 'ARG-1')]), '2 tickets');
});

test('only a waiting proposal asks for attention at Reconcile', () => {
  const base = { reconciled: 'reconciled 3d ago by pane #32', changes: 11 };
  const waits = reconcileEntry({ ...base, reconcile: 'proposed' });
  assert.equal(waits.label, 'Reconcile — a proposal waits');
  assert.equal(waits.attention, true);
  assert.equal(waits.hint, 'Reconciled 3d ago · 11 changes since');
  assert.equal(reconcileEntry({ ...base, reconcile: 'asked' }).attention, false);
  const idle = reconcileEntry({ reconcile: null, reconciled: null, changes: null });
  assert.equal(idle.label, 'Reconcile with its tickets…');
  assert.equal(idle.hint, 'never reconciled');
});
