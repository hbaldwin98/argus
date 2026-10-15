// The saved views' rules under node: when an embed is drawn, how the
// sidebar groups and filters, and which views a feature offers.

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { embedVerdict, forFeature, grouped, MAX_EMBED_DEPTH } from '../src/saved_logic.ts';

const held = (ids) => (id) => ids.includes(id);

test('an embed of a view already being drawn is a cycle, whatever the depth', () => {
  assert.equal(embedVerdict('a', ['a'], 0, held(['a'])), 'cycle');
  assert.equal(embedVerdict('a', ['a', 'b', 'c'], 2, held(['a'])), 'cycle');
});

test('an embed is drawn down to the depth cap and a line says so past it', () => {
  const all = held(['b']);
  assert.equal(embedVerdict('b', [], 0, all), 'draw');
  assert.equal(embedVerdict('b', ['x'], MAX_EMBED_DEPTH - 1, all), 'draw');
  assert.equal(embedVerdict('b', ['x'], MAX_EMBED_DEPTH, all), 'deep');
});

test('an embed of a view the window does not hold is missing', () => {
  assert.equal(embedVerdict('later', ['a'], 1, held(['a'])), 'missing');
});

const view = (id, label, here, features = []) => ({
  id,
  scope: { rank: 0, label },
  here,
  features,
});

test('the sidebar groups consecutive views by scope and keeps only the ones here when asked', () => {
  const views = [view('a', 'Global', true), view('b', 'argus', false), view('c', 'argus', true)];
  assert.deepEqual(
    grouped(views, false).map((g) => [g.label, g.views.map((v) => v.id)]),
    [
      ['Global', ['a']],
      ['argus', ['b', 'c']],
    ],
  );
  assert.deepEqual(
    grouped(views, true).map((g) => [g.label, g.views.map((v) => v.id)]),
    [
      ['Global', ['a']],
      ['argus', ['c']],
    ],
  );
});

test("a feature's stage offers the views scoped or attached to that feature of its repository", () => {
  const views = [
    view('a', 'x', true, [{ repository: 10, number: 7 }]),
    view('b', 'x', true, [{ repository: 11, number: 7 }]),
    view('c', 'x', true, [{ repository: 10, number: 8 }]),
  ];
  assert.deepEqual(
    forFeature(views, 10, 7).map((v) => v.id),
    ['a'],
  );
  assert.deepEqual(forFeature(views, null, 7), []);
});
