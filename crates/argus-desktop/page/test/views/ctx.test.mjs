// `views/ctx` under node: a view context says whose view it is, and only an
// agent's names a pane.

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { at, paneOf } from '../../src/views/ctx.ts';

const base = { id: 'ci', path: '3/ci', drawn: () => undefined, canPress: true };

test('only an agents view names the pane its references are read against', () => {
  assert.equal(paneOf({ ...base, kind: 'agent', pane: 3 }), 3);
  assert.equal(
    paneOf({ ...base, kind: 'saved', pane: 3, chain: ['ci'] }),
    null,
    'a saved view has none, whatever its pane says',
  );
  assert.equal(paneOf(null), null);
});

test('a blocks context keeps the kind of the view it is in', () => {
  const inner = at({ ...base, kind: 'saved', pane: 0, chain: ['ci'] }, 2);
  assert.equal(inner.kind, 'saved');
  assert.equal(inner.path, '3/ci.2');
});
