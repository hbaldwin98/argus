// `checkouts/says` under node: what the table's State cell and the name
// column say of a row, and above all when they say nothing.

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { branchBeside, stateOf } from '../../src/checkouts/says.ts';

const row = (over = {}) => ({
  key: 'c1',
  checkout: 1,
  name: 'argus-s2',
  primary: false,
  branch: 'argus-s2',
  git: 'clean',
  clean: true,
  changed: 0,
  ahead: 0,
  behind: 0,
  detached: false,
  note: null,
  last: null,
  running: [],
  shared: null,
  feature: null,
  viewed: false,
  ...over,
});

test('a clean checkout in step with its upstream and idle says nothing', () => {
  assert.deepEqual(stateOf(row()), []);
});

test('changes, distance, a detached head, panes and a feature are each said, in order', () => {
  const says = stateOf(
    row({
      detached: true,
      changed: 3,
      ahead: 2,
      behind: 1,
      running: [
        { id: 1, title: 'a', agent: true, status: 'working' },
        { id: 2, title: 'b', agent: false, status: 'idle' },
      ],
      feature: 7,
    }),
  );
  assert.deepEqual(
    says.map((s) => s.kind),
    ['detached', 'changed', 'distance', 'panes', 'feature'],
  );
  assert.equal(says[1].text, '3 changed');
  assert.equal(says[2].text, '↑2 ↓1');
  assert.equal(says[3].count, 2);
});

test('only the side of the upstream that differs is drawn', () => {
  assert.equal(stateOf(row({ behind: 4 }))[0].text, '↓4');
  assert.equal(stateOf(row({ ahead: 1 }))[0].text, '↑1');
});

test('the branch shows beside the name only when it differs from the folder', () => {
  assert.equal(branchBeside(row()), null);
  assert.equal(branchBeside(row({ branch: 'feature/x' })), 'feature/x');
});

test('a detached head is not a branch to name, and a branch row has no folder name', () => {
  assert.equal(branchBeside(row({ detached: true, branch: 'detached' })), null);
  assert.equal(branchBeside(row({ name: null, branch: 'old' })), null);
});
