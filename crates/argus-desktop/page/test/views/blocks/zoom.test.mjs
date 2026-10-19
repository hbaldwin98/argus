// `views/blocks/zoom` under node: how a drawing opens in the larger view
// and how the wheel moves it, as the person asked for in the change that
// added it (a graph unreadable in a side panel, opened larger to zoom and
// drag).

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { fit, MAX_SCALE, MIN_SCALE, start, zoomAt } from '../../../src/views/blocks/zoom.ts';

const near = (a, b) => Math.abs(a - b) < 1e-6;

test('a fitted drawing is centred and inside the stage', () => {
  const z = fit(1000, 400, 800, 600);
  assert.ok(1000 * z.scale <= 800 && 400 * z.scale <= 600);
  assert.ok(near(z.x * 2 + 1000 * z.scale, 800));
  assert.ok(near(z.y * 2 + 400 * z.scale, 600));
});

test('a long sequence opens across the stage from its top rather than shrunk whole', () => {
  const z = start(900, 4000, 1200, 800);
  assert.ok(z.scale > fit(900, 4000, 1200, 800).scale);
  assert.ok(z.y > 0 && z.y < 100, 'starts at the top');
});

test('a drawing readable whole opens whole', () => {
  assert.deepEqual(start(600, 400, 1200, 800), fit(600, 400, 1200, 800));
});

test('the point under the wheel stays put while zooming, and the scale is bounded', () => {
  for (const factor of [0.5, 0.9, 1.1, 3]) {
    const z = { scale: 0.8, x: 40, y: -120 };
    const [px, py] = [300, 250];
    const before = [(px - z.x) / z.scale, (py - z.y) / z.scale];
    const after = zoomAt(z, factor, px, py);
    assert.ok(near((px - after.x) / after.scale, before[0]));
    assert.ok(near((py - after.y) / after.scale, before[1]));
  }
  assert.equal(zoomAt({ scale: 1, x: 0, y: 0 }, 1000, 0, 0).scale, MAX_SCALE);
  assert.equal(zoomAt({ scale: 1, x: 0, y: 0 }, 0.0001, 0, 0).scale, MIN_SCALE);
});
