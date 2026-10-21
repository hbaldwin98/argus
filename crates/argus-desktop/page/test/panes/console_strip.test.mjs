// `panes/console_strip`: one menu holds split, move and fill; idle shells wear no dot.

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { stripItems, wearsDot } from '../../src/panes/console_strip.ts';

const on = { split: false, docked: true, full: false, dock: 'at the bottom' };

test('the menu holds split, move and fill the window', () => {
  assert.deepEqual(
    stripItems(on).map((i) => i.action),
    ['split', 'tab', 'fullscreen'],
  );
});

test('the menu names the way back once split, undocked or full', () => {
  const items = stripItems({ split: true, docked: false, full: true, dock: 'at the bottom' });
  assert.deepEqual(
    items.map((i) => [i.action, i.label]),
    [
      ['split', 'Put the split away'],
      ['dock', 'Dock at the bottom'],
      ['fullscreen', 'Leave fullscreen'],
    ],
  );
});

test('an idle shell wears no dot, a busy or failed one does', () => {
  assert.equal(wearsDot('idle'), false);
  for (const s of ['working', 'waiting', 'needs-review', 'failed', 'done', 'exited']) {
    assert.equal(wearsDot(s), true, s);
  }
});
