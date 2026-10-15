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

/** Rust's `char::is_control`: the C0 and C1 control characters. */
function isControl(c: string): boolean {
  const code = c.codePointAt(0) ?? 0;
  return code < 0x20 || (code >= 0x7f && code <= 0x9f);
}

/** Whether a path is drawn as a button rather than as plain text. This is
 *  `is_checkout_relative` in `argus-protocol/src/checkout_path.rs` written a
 *  second time, since a button is drawn without asking Rust; Rust is the
 *  authority and refuses again when one is pressed, so a drift here costs a
 *  button that says no, never an escape. Change the two together. */
function inside(path: string): boolean {
  if (path === '' || /^[/\\]/.test(path) || Array.from(path).some(isControl)) return false;
  if (/^[A-Za-z]:/.test(path)) return false;
  let named = false;
  for (const part of path.split(/[/\\]/)) {
    if (part === '' || part === '.') continue;
    // Windows drops trailing dots and spaces from a name, so `.. ` is `..`.
    if (/^\.+$/.test(part) || /[. ]$/.test(part)) return false;
    named = true;
  }
  return named;
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
