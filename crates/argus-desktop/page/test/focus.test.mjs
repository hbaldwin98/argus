// `focus` under node, with the layers' predicates stubbed: what it answers
// is decided by the order it is handed and nothing of its own.

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { escapeFrom, takesFromPane, up } from '../src/focus.ts';

const order = [
  { layer: 'field', places: null, takesFromPane: false },
  { layer: 'review', places: ['anywhere', 'review'], takesFromPane: true },
  { layer: 'history', places: ['anywhere', 'history'], takesFromPane: true },
  { layer: 'menu', places: ['anywhere', 'menu'], takesFromPane: false },
  { layer: 'stage', places: ['anywhere'], takesFromPane: false },
];

/** Handlers for `order`, each up when named in `open`, Escape putting
 *  away those named in `escapes` and saying so in `escaped`. */
function handlers(open, escapes = [], escaped = []) {
  return Object.fromEntries(
    order.map(({ layer }) => [
      layer,
      {
        open: () => open.includes(layer),
        escape: () => {
          if (!escapes.includes(layer)) return false;
          escaped.push(layer);
          return true;
        },
      },
    ]),
  );
}

const names = (layers) => [...layers].map((l) => l.layer);

test('the first layer up in the order has the keys', () => {
  assert.deepEqual(names(up(order, handlers(['stage', 'history', 'review']), null)), [
    'review',
    'history',
    'stage',
  ]);
  assert.deepEqual(names(up(order, handlers(['stage']), null)), ['stage']);
});

test('a layer that answers its own keys is up like any other', () => {
  const [first] = up(order, handlers(['field', 'review', 'stage']), null);
  assert.equal(first.layer, 'field');
  assert.equal(first.places, null);
});

test('the layers are asked only as far as the walk goes', () => {
  const asked = [];
  const lazy = Object.fromEntries(
    order.map(({ layer }) => [layer, { open: () => (asked.push(layer), layer === 'review') }]),
  );
  for (const layer of up(order, lazy, null)) {
    assert.equal(layer.layer, 'review');
    break;
  }
  assert.deepEqual(asked, ['field', 'review']);
});

test('the predicates hear where the keys are', () => {
  const target = {};
  const seen = [];
  const at = Object.fromEntries(
    order.map(({ layer }) => [layer, { open: (t) => (seen.push(t), false) }]),
  );
  assert.deepEqual(names(up(order, at, target)), []);
  assert.ok(seen.every((t) => t === target));
});

test('only a layer marked so takes the keys from a pane', () => {
  assert.equal(takesFromPane(order, handlers(['history']), null), true);
  assert.equal(takesFromPane(order, handlers(['review', 'stage']), null), true);
  assert.equal(takesFromPane(order, handlers(['field', 'menu', 'stage']), null), false);
  assert.equal(takesFromPane(order, handlers([]), null), false);
});

test('escape puts away the first thing in the order, up or not', () => {
  const escaped = [];
  assert.equal(escapeFrom(order, handlers([], ['stage', 'menu'], escaped)), true);
  assert.deepEqual(escaped, ['menu']);
  assert.equal(escapeFrom(order, handlers(['review'], [])), false);
  const bare = Object.fromEntries(order.map(({ layer }) => [layer, { open: () => true }]));
  assert.equal(escapeFrom(order, bare), false);
});
