// The saved views' rules under node: when an embed is drawn, how the
// sidebar groups, nests and filters, and which views a feature offers.

import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  count,
  embedVerdict,
  forFeature,
  grouped,
  MAX_EMBED_DEPTH,
  nested,
  underGroup,
} from '../../../src/views/saved/logic.ts';

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

const under = (id, parent) => ({ id, under: parent });
const shape = (nodes) => nodes.map((n) => [n.view.id, shape(n.inner)]);

test('the sidebar lists a view under the one Rust names, at any depth, in order', () => {
  const top = nested([under('a', null), under('b', 'a'), under('c', 'b'), under('d', 'a')]);
  assert.deepEqual(shape(top), [
    [
      'a',
      [
        ['b', [['c', []]]],
        ['d', []],
      ],
    ],
  ]);
  assert.equal(count(top[0]), 3);
});

test('a view whose parent is filtered out stands at the top', () => {
  assert.deepEqual(shape(nested([under('b', 'a'), under('c', 'b')])), [['b', [['c', []]]]]);
});

test('the sidebar drops a title that opens with its group label, but never to nothing', () => {
  assert.equal(underGroup('reliab: the change flow', 'reliab'), 'the change flow');
  assert.equal(underGroup('Reliab — the change flow', 'reliab'), 'the change flow');
  assert.equal(underGroup('reliab', 'reliab'), 'reliab');
  assert.equal(underGroup('reliab:', 'reliab'), 'reliab:');
  assert.equal(underGroup('reliability report', 'reliab'), 'reliability report');
  assert.equal(underGroup('the change flow', 'reliab'), 'the change flow');
  assert.equal(underGroup('a: b', ''), 'a: b');
});
