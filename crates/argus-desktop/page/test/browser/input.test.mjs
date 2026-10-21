// The person's hand on a browser's picture under node: where a click lands
// in the page, which keys are the browser's, and what an address means.

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { address, keyInput, within } from '../../src/browser/input.ts';

const page = { width: 1280, height: 800 };

test('a click lands where it was aimed however the picture is fitted', () => {
  // Wider than the page's shape: bands left and right.
  const wide = { left: 100, top: 50, width: 2000, height: 800 };
  assert.deepEqual(within(wide, page, 100 + 360 + 640, 50 + 400), { x: 640, y: 400 });
  assert.deepEqual(within(wide, page, 100 + 360, 50), { x: 0, y: 0 });
  // Taller: bands above and below, the picture at half size.
  const tall = { left: 0, top: 0, width: 640, height: 1000 };
  assert.deepEqual(within(tall, page, 320, 300 + 200), { x: 640, y: 400 });
});

test('a click in a band beside the picture is not sent', () => {
  const wide = { left: 0, top: 0, width: 2000, height: 800 };
  assert.equal(within(wide, page, 100, 400), null);
  assert.equal(within(wide, page, 1900, 400), null);
});

const key = (k, mods = {}) =>
  keyInput({ key: k, ctrlKey: false, metaKey: false, altKey: false, ...mods });

test('a chord is the window’s, a named key or a letter the browser’s', () => {
  assert.deepEqual(key('Enter'), { kind: 'key', key: 'Enter' });
  assert.deepEqual(key('a'), { kind: 'text', text: 'a' });
  assert.deepEqual(key('6'), { kind: 'text', text: '6' });
  assert.equal(key('k', { ctrlKey: true }), null);
  assert.equal(key('Shift'), null);
  assert.equal(key('F5'), null);
});

test('a bare host is taken as the web, and a scheme is kept as typed', () => {
  assert.equal(address(' example.com/a '), 'https://example.com/a');
  assert.equal(address('http://localhost:3000'), 'http://localhost:3000');
  // The daemon refuses it; the page does not dress it up as something else.
  assert.equal(address('file:///etc/passwd'), 'file:///etc/passwd');
});
