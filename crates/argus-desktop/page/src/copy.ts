// Putting text on the clipboard. Rust writes it: the page is never granted
// the webview's clipboard, as a paste is read in Rust too.

import { el } from './dom.js';
import { call } from './tauri.js';

/** A button that copies `text()`, read at the press, and says so for a moment. */
export function copyButton(label: string, text: () => string): HTMLElement {
  const button = el('button', { class: 'btn sm vcopy', type: 'button' }, label);
  let timer: number | undefined;
  button.addEventListener('click', (e) => {
    e.stopPropagation();
    call('copy_text', { text: text() });
    button.textContent = 'Copied';
    window.clearTimeout(timer);
    timer = window.setTimeout(() => {
      button.textContent = label;
    }, 1200);
  });
  return button;
}
