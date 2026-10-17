// A log view block: a tail of lines, each with its level.

import { el } from '../../kit/dom.js';
import { ago } from '../values.js';
import type { Block } from '../../bindings/Block';
import type { ViewCtx } from '../ctx.js';

// Where the person left each log, by block path. A log is a tail, so one
// they have not scrolled up in follows the newest line; one they have keeps
// its place across a redraw.
const scrolled = new Map<string, { top: number; tail: boolean }>();

export function log(block: Extract<Block, { kind: 'log' }>, ctx: ViewCtx): HTMLElement {
  const box = el(
    'div',
    { class: 'vlog', tabindex: '0', role: 'log' },
    block.lines.map((line) =>
      el(
        'div',
        { class: `vlog-line vlog-${line.level}` },
        line.at != null && el('span', { class: 'vlog-at' }, ago(line.at)),
        el('span', { class: 'vlog-level' }, line.level),
        el('span', { class: 'vlog-text' }, line.text),
      ),
    ),
  );
  const mine = scrolled.get(ctx.path);
  // The box is not in the page yet, so it has no height to scroll to.
  requestAnimationFrame(() => {
    box.scrollTop = !mine || mine.tail ? box.scrollHeight : mine.top;
    box.addEventListener('scroll', () => {
      const tail = box.scrollHeight - box.scrollTop - box.clientHeight < 4;
      scrolled.set(ctx.path, { top: box.scrollTop, tail });
    });
  });
  return box;
}
