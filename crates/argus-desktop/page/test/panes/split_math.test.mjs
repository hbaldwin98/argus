// The Workspace grid's arithmetic under node: rectangles from fractions,
// what a gutter drag does to its two panes, and which zone a drop is over.

import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  cellRects,
  dragGutter,
  dropZone,
  keyedSplit,
  staleView,
  toThousandths,
  zoneRect,
} from '../../src/panes/split_math.ts';

test('rectangles tile the grid row by row from the heights and widths', () => {
  const rects = cellRects([250, 750], [[400, 600], [1000]]);
  assert.deepEqual(rects[0], [
    { x: 0, y: 0, w: 0.4, h: 0.25 },
    { x: 0.4, y: 0, w: 0.6, h: 0.25 },
  ]);
  assert.deepEqual(rects[1], [{ x: 0, y: 0.25, w: 1, h: 0.75 }]);
});

test('dragging a gutter moves the edge between its two panes and leaves the rest', () => {
  assert.deepEqual(dragGutter([300, 300, 300], 0, 50, 100), [350, 250, 300]);
  assert.deepEqual(dragGutter([300, 300, 300], 1, -50, 100), [300, 250, 350]);
});

test('a drag stops at the minimum of the pane that is shrinking', () => {
  assert.deepEqual(dragGutter([300, 300, 300], 0, 500, 100), [500, 100, 300]);
  assert.deepEqual(dragGutter([300, 300, 300], 0, -500, 100), [100, 500, 300]);
});

test('a pair with no room for two minimums does not move', () => {
  assert.deepEqual(dragGutter([90, 90], 0, 40, 100), [90, 90]);
  assert.deepEqual(dragGutter([150], 0, 40, 10), [150]);
});

test('sizes become thousandths that sum to exactly one thousand', () => {
  assert.deepEqual(toThousandths([1, 1, 1]), [334, 333, 333]);
  assert.deepEqual(toThousandths([300, 700]), [300, 700]);
  assert.equal(
    toThousandths([123, 456, 789, 11]).reduce((a, b) => a + b, 0),
    1000,
  );
  assert.deepEqual(toThousandths([0, 0]), [500, 500]);
});

test('a header over the middle of a pane swaps; near an edge it goes there', () => {
  assert.equal(dropZone(0.5, 0.5), 'centre');
  assert.equal(dropZone(0.1, 0.5), 'left');
  assert.equal(dropZone(0.9, 0.5), 'right');
  assert.equal(dropZone(0.5, 0.05), 'top');
  assert.equal(dropZone(0.5, 0.95), 'bottom');
  assert.equal(dropZone(0.1, 0.05), 'top', 'the nearer edge in a corner');
});

test('the lit zone is the half it would take, or the whole pane to swap', () => {
  assert.deepEqual(zoneRect('left'), { x: 0, y: 0, w: 0.5, h: 1 });
  assert.deepEqual(zoneRect('bottom'), { x: 0, y: 0.5, w: 1, h: 0.5 });
  assert.deepEqual(zoneRect('centre'), { x: 0, y: 0, w: 1, h: 1 });
});

test("the keys on a split name its screen, and on the Workspace's or off the grid none", () => {
  assert.equal(keyedSplit(null), null);
  assert.equal(keyedSplit(0), null);
  assert.equal(keyedSplit(3), 2);
});

test('a view that moved to an earlier cell is not taken out of it by the later cell', () => {
  const box0 = { id: 'cell 0' };
  const box1 = { id: 'cell 1' };
  const view = { parentElement: box0 };
  // Cell 0 took the view first; cell 1 still holds it as its old one.
  assert.equal(staleView(view, null, box1), null);
  assert.equal(staleView(view, null, box0), view, 'its own cell gives it up');
  assert.equal(staleView(view, view, box0), null, 'a view kept is not taken out');
  assert.equal(staleView(null, view, box0), null);
});
