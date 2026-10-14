// What the page asks of the window's Rust, and the channel its events come
// down. Every command here is one in `src/commands.rs`; the events are
// `src/page.rs`'s, typed by the bindings `cargo test` writes.

import type { Channel as TauriChannel, invoke as tauriInvoke } from '@tauri-apps/api/core';
import type { Event } from './bindings/Event';
import type { PageKey } from './bindings/PageKey';
import type { PageMouse } from './bindings/PageMouse';

declare global {
  interface Window {
    __TAURI__: { core: { invoke: typeof tauriInvoke; Channel: typeof TauriChannel } };
  }
}

type None = Record<string, never>;

interface Commands {
  connect: { events: TauriChannel<Event> };
  select: { pane: number };
  resize: { rows: number; cols: number };
  key: { key: PageKey };
  text: { text: string };
  paste: { text: string };
  paste_clipboard: None;
  wheel: { notches: number; at: [number, number] };
  mouse: { gesture: PageMouse; button: number; at: [number, number] };
  copy: { from: [number, number]; to: [number, number] };
  spawn_shell: { checkout: number };
  close: { pane: number };
  focus: { focused: boolean };
  select_project: { project: number };
  toggle_repository: { repository: number };
  choose_repository: { repository: number };
  show_features: { open: boolean };
  peek: { slug: string };
  set_layout: { layout: 'list' | 'board' };
  toggle_folded: None;
  hold: { slug: string; reason: string | null };
  archive: { slug: string; archived: boolean };
  go_to_pane: { pane: number };
  reconnect: None;
}

/** Sends a call. It answers nothing: what it changed comes back as events. */
export function call<C extends keyof Commands>(command: C, args: Commands[C]): void {
  window.__TAURI__.core.invoke(command, args).catch((err: unknown) => {
    console.error(command, err);
  });
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
  call('connect', { events });
}
