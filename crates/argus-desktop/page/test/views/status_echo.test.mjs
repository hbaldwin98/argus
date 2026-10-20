// `extensions/status_echo`: a cell that repeats the row's status says nothing new.

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { echoes } from '../../src/extensions/status_echo.ts';

const row = (label) => ({ status: label === null ? null : { label, tone: 'neutral' } });

test('brackets and case do not hide a cell that repeats the status', () => {
  assert.equal(echoes(row('exited (137)'), { kind: 'text', text: 'exited 137' }), true);
  assert.equal(echoes(row('Running'), { kind: 'chip', chip: { label: ' running ' } }), true);
});

test('a cell that says more than the status is not an echo', () => {
  assert.equal(echoes(row('exited'), { kind: 'text', text: 'exited 137' }), false);
  assert.equal(echoes(row(null), { kind: 'text', text: 'exited' }), false);
  assert.equal(echoes(row('exited'), undefined), false);
});
