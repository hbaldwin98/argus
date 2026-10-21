// `panes/size_label`: the header names the grid's size only just after it changed.

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { sizeLabel } from '../../src/panes/size_label.ts';

const at = { cols: 140, rows: 40, parked: null, shownUntil: 1000 };

test('the size is silent at rest and shows just after a resize', () => {
  assert.equal(sizeLabel({ ...at, now: 1500 }), '');
  assert.equal(sizeLabel({ ...at, now: 500 }), '140 × 40');
});

test('a parked view keeps its place at rest and leads the size while it shows', () => {
  const parked = { offset: 12, depth: 300 };
  assert.equal(sizeLabel({ ...at, parked, now: 1500 }), '↑ 12/300');
  assert.equal(sizeLabel({ ...at, parked, now: 500 }), '↑ 12/300 · 140 × 40');
});

test('a pane with no grid yet says nothing', () => {
  assert.equal(sizeLabel({ ...at, cols: 0, now: 0 }), '');
});
