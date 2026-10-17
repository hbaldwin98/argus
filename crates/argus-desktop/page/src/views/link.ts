// A link in a view an agent showed. The page never navigates and never
// hands a URL to the webview: a click asks Rust to give it to the system's
// opener, which opens only an `http` or `https` one. The URL is an agent's,
// read from a ticket, so it is kept as data and shown nowhere but the
// tooltip.

import { el, type Child } from '../kit/dom.js';
import { call } from '../connection/tauri.js';

export function viewLink(url: string, ...label: Child[]): HTMLElement {
  const open = (event: Event): void => {
    event.preventDefault();
    event.stopPropagation();
    call('open_url', { url });
  };
  // A button rather than an anchor: it takes Enter and Space without a
  // key handler of its own, and there is no href for the webview to follow.
  return el('button', { class: 'vlink', type: 'button', title: url, onclick: open }, label);
}
