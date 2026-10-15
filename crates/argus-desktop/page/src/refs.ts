// The values that point into Argus rather than out of it — a path, a
// commit, a feature, a pane — and the plain ones drawn beside them: a
// duration and inline code. A pointer is a small button that opens its
// target in this window, never in a browser; Rust finds the target from the
// pane that showed the view, so the page names the reference and nothing
// more.

import type { Reference } from './bindings/Reference';
import type { Value } from './bindings/Value';
import { el } from './dom.js';
import { call } from './tauri.js';

export type RefValue = Extract<
  Value,
  | { kind: 'path' }
  | { kind: 'commit' }
  | { kind: 'feature' }
  | { kind: 'pane' }
  | { kind: 'duration' }
  | { kind: 'code' }
>;

const SECOND = 1000;
const MINUTE = 60 * SECOND;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

/** A span of time in the two units that matter: `850 ms`, `4m 12s`, `1h 3m`. */
export function duration(ms: number): string {
  if (ms < SECOND) return `${Math.round(ms)} ms`;
  if (ms < MINUTE) return `${Math.floor(ms / SECOND)}s`;
  if (ms < HOUR) return `${Math.floor(ms / MINUTE)}m ${Math.floor((ms % MINUTE) / SECOND)}s`;
  if (ms < DAY) return `${Math.floor(ms / HOUR)}h ${Math.floor((ms % HOUR) / MINUTE)}m`;
  return `${Math.floor(ms / DAY)}d ${Math.floor((ms % DAY) / HOUR)}h`;
}

/** Whether a path stays inside a checkout. Rust refuses the rest again when
 *  it is pressed; this only decides whether it is drawn as a button. */
function inside(path: string): boolean {
  return (
    path !== '' &&
    !path.includes('\0') &&
    !/^[A-Za-z]:/.test(path) &&
    !/^[/\\]/.test(path) &&
    !path.split(/[/\\]/).includes('..')
  );
}

function pointer(
  text: string,
  title: string,
  pane: number | null,
  reference: Reference,
): HTMLElement {
  return el(
    'button',
    {
      class: 'vref',
      type: 'button',
      title,
      onclick: (event: MouseEvent) => {
        event.stopPropagation();
        call('open_ref', { pane, reference });
      },
    },
    text,
  );
}

/** One of the values above. `pane` is the one whose agent showed the view,
 *  and none for an extension's table. */
export function refValue(v: RefValue, pane: number | null): HTMLElement {
  switch (v.kind) {
    case 'duration':
      return el('span', { class: 'vnum', title: `${v.ms} ms` }, duration(v.ms));
    case 'code':
      return el('code', { class: 'vicode' }, v.text);
    case 'path': {
      const text = v.line == null ? v.path : `${v.path}:${v.line}`;
      if (!inside(v.path)) return el('span', { class: 'vicode' }, text);
      return pointer(text, 'Open in your editor', pane, {
        kind: 'path',
        path: v.path,
        line: v.line ?? null,
      });
    }
    case 'commit':
      return el(
        'span',
        { class: 'vticket' },
        pointer(v.sha.slice(0, 7), 'Show in History', pane, { kind: 'commit', sha: v.sha }),
        v.title && el('span', { class: 'vmuted' }, v.title),
      );
    case 'feature':
      return pointer(`#${v.number}`, 'Show in the Feature stage', pane, {
        kind: 'feature',
        number: v.number,
      });
    case 'pane':
      return pointer(v.label ?? `pane #${v.pane}`, 'Go to the pane', pane, {
        kind: 'pane',
        pane: v.pane,
      });
  }
}
