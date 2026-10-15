// What the page asks of the window's Rust, and the channel its events come
// down. Every call is one `Call` of `src/page.rs`, which `src/commands.rs`
// dispatches, and the events are that file's too: both are typed by the
// bindings `cargo test` writes.

import type { Channel as TauriChannel, invoke as tauriInvoke } from '@tauri-apps/api/core';
import type { Call } from './bindings/Call';
import type { Event } from './bindings/Event';

declare global {
  interface Window {
    __TAURI__: { core: { invoke: typeof tauriInvoke; Channel: typeof TauriChannel } };
  }
}

/** A call's name, as `Call` tags it. */
export type CallName = Call['call'];

/** What a call carries beside its name. */
export type CallArgs<C extends CallName> = Omit<Extract<Call, { call: C }>, 'call'>;

function invoke(command: string, args: Record<string, unknown>): void {
  window.__TAURI__.core.invoke(command, args).catch((err: unknown) => {
    console.error(command, args, err);
  });
}

/** Sends a call. It answers nothing: what it changed comes back as events. */
export function call<C extends CallName>(name: C, args: CallArgs<C>): void {
  invoke('call', { call: { call: name, ...args } });
}

/** Opens the page's channel; Rust sends everything it needs at once. */
export function connect(receive: (event: Event) => void): void {
  const events = new window.__TAURI__.core.Channel<Event>();
  // One bad message is logged rather than ending the stream. A Channel
  // offers only this setter, not addEventListener.
  // oxlint-disable-next-line unicorn/prefer-add-event-listener
  events.onmessage = (event) => {
    try {
      receive(event);
    } catch (err) {
      console.error(err);
    }
  };
  invoke('connect', { events });
}
