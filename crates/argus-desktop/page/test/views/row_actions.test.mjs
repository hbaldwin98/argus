// `extensions/row_actions`: a row shows its actions only while selected or hovered.

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { keyed, showsActions } from '../../src/extensions/row_actions.ts';

test('an unselected, unhovered row shows no actions', () => {
  assert.equal(showsActions('a', 'b', null), false);
  assert.equal(showsActions('a', null, 'b'), false);
});

test('the selected row and the hovered row show them', () => {
  assert.equal(showsActions('a', 'a', null), true);
  assert.equal(showsActions('a', 'b', 'a'), true);
});

test('a row with no key never does', () => {
  assert.equal(showsActions(null, null, null), false);
});

const act = (id, key) => ({ id, label: id, key, viewWide: false });

test('only actions with a key are listed under Keys', () => {
  assert.deepEqual(
    keyed([act('logs', 'l'), act('inspect', null)]).map((a) => a.id),
    ['logs'],
  );
});
