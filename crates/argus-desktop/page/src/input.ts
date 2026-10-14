// The pane's keys, text and pastes, taken by a hidden text area so input
// methods and the clipboard work as they do anywhere else, and the
// window's focus, which says whether this window's view is the person's.

import { $ } from './dom.js';
import { actionFor } from './keymap.js';
import { screen } from './screen.js';
import { call } from './tauri.js';

const MODIFIERS = new Set(['Shift', 'Control', 'Alt', 'Meta', 'AltGraph', 'CapsLock']);
const MAC = /Macintosh|Mac OS X/.test(navigator.userAgent);

export function listen(): void {
  const input = $('input');
  if (!(input instanceof HTMLTextAreaElement)) throw new Error('#input is not a text area');

  input.addEventListener('keydown', (e) => {
    // WebKit reports an input method's keys as 229 without isComposing.
    if (e.isComposing || e.keyCode === 229) return;
    // The command key's chords are the window's (quit, close), but for
    // those the keymap gives the pane: paste.
    if (MAC && e.metaKey && !actionFor(['pane'], e)) return;
    if (MODIFIERS.has(e.key)) return;
    screen.select(null);
    // Every other key goes to Rust, which dispatches it from the keymap:
    // the leader and the paste chord are the window's, the rest the
    // pane's. No webview accelerator (F5, Ctrl+F, Ctrl+R) fires behind the
    // person's back.
    e.preventDefault();
    call('key', {
      key: {
        key: e.key,
        code: e.code,
        ctrl: e.ctrlKey,
        alt: e.altKey,
        shift: e.shiftKey,
        meta: e.metaKey,
      },
    });
  });

  input.addEventListener('compositionend', (e) => {
    if (e.data) call('text', { text: e.data });
    input.value = '';
  });

  input.addEventListener('input', (e) => {
    if (!(e instanceof InputEvent) || !e.isComposing) input.value = '';
  });

  input.addEventListener('paste', (e) => {
    e.preventDefault();
    const text = e.clipboardData?.getData('text/plain');
    if (text) call('paste', { text });
  });

  input.addEventListener('focus', () => screen.drawCursor());
  input.addEventListener('blur', () => screen.drawCursor());
  // A drag selects, and letting go copies, as in the terminal client. A
  // child that asked for the mouse gets its presses, drags and moves
  // instead, as Rust encodes them for its mode; Shift-drag still selects.
  let anchor: [number, number] | null = null;
  let moved = false;
  let held: number | null = null;
  let last: [number, number] | null = null;
  const report = (
    gesture: 'down' | 'up' | 'drag' | 'move',
    button: number,
    at: [number, number],
  ): void => {
    last = at;
    call('mouse', { gesture, button, at });
  };
  const moving = (at: [number, number]): boolean => !last || last[0] !== at[0] || last[1] !== at[1];

  $('terminal').addEventListener('mousedown', (e) => {
    e.preventDefault();
    input.focus();
    const at = screen.cellAt(e.clientX, e.clientY);
    if (screen.takesMouse() && !e.shiftKey) {
      held = e.button;
      report('down', e.button, at);
      return;
    }
    if (e.button !== 0) return;
    anchor = at;
    moved = false;
    screen.select(null);
  });
  $('terminal').addEventListener('contextmenu', (e) => e.preventDefault());
  $('terminal').addEventListener('mousemove', (e) => {
    if (anchor || held !== null || !screen.takesMouse()) return;
    const at = screen.cellAt(e.clientX, e.clientY);
    if (moving(at)) report('move', 0, at);
  });
  window.addEventListener('mousemove', (e) => {
    if (held !== null) {
      const at = screen.cellAt(e.clientX, e.clientY);
      if (moving(at)) report('drag', held, at);
      return;
    }
    if (!anchor) return;
    const head = screen.cellAt(e.clientX, e.clientY);
    moved ||= head[0] !== anchor[0] || head[1] !== anchor[1];
    if (moved) screen.select({ anchor, head });
  });
  window.addEventListener('mouseup', (e) => {
    if (held !== null) {
      if (e.button === held) {
        report('up', held, screen.cellAt(e.clientX, e.clientY));
        held = null;
      }
      return;
    }
    if (!anchor) return;
    if (moved) call('copy', { from: anchor, to: screen.cellAt(e.clientX, e.clientY) });
    anchor = null;
  });

  // Rust decides what a notch is to the pane. A notch of a physical wheel
  // is 100 pixels or 3 lines; a touchpad's small deltas add up to one.
  let wheel = 0;
  $('terminal').addEventListener(
    'wheel',
    (e) => {
      e.preventDefault();
      wheel += e.deltaMode === WheelEvent.DOM_DELTA_LINE ? e.deltaY / 3 : e.deltaY / 100;
      const notches = Math.trunc(wheel);
      if (!notches) return;
      wheel -= notches;
      call('wheel', { notches, at: screen.cellAt(e.clientX, e.clientY) });
    },
    { passive: false },
  );

  window.addEventListener('focus', () => {
    call('focus', { focused: true });
    screen.drawCursor();
  });
  window.addEventListener('blur', () => {
    call('focus', { focused: false });
    screen.drawCursor();
  });

  // A pane appearing after the empty stage needs its room reported.
  new MutationObserver(() => screen.report()).observe($('terminal'), {
    attributes: true,
    attributeFilter: ['hidden'],
  });
}
