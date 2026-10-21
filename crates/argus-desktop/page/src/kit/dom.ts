// Building the page's elements. Text from the daemon — titles, notes,
// names — only ever goes in as text; markup is only ever the icons below.

export const $ = (id: string): HTMLElement => {
  const node = document.getElementById(id);
  if (!node) throw new Error(`the page has no #${id}`);
  return node;
};

const ICON = {
  more: '<svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true" fill="currentColor"><circle cx="3.5" cy="8" r="1.3"></circle><circle cx="8" cy="8" r="1.3"></circle><circle cx="12.5" cy="8" r="1.3"></circle></svg>',
  down: '<svg width="12" height="12" viewBox="0 0 14 14" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M3.5 5.5L7 9l3.5-3.5"></path></svg>',
  search:
    '<svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><circle cx="7" cy="7" r="4.5"></circle><path d="M10.5 10.5L14 14"></path></svg>',
  refresh:
    '<svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M13 8a5 5 0 1 1-1.5-3.5M13 2.5v2.5h-2.5"></path></svg>',
  up: '<svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M8 13V3M3.5 7.5L8 3l4.5 4.5"></path></svg>',
  right:
    '<svg width="12" height="12" viewBox="0 0 14 14" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M5.5 3.5L9 7l-3.5 3.5"></path></svg>',
  branch:
    '<svg width="12" height="12" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="4.5" cy="3.5" r="1.5"></circle><circle cx="4.5" cy="12.5" r="1.5"></circle><circle cx="11.5" cy="5.5" r="1.5"></circle><path d="M4.5 5v6M11.5 7c0 3-7 2-7 4"></path></svg>',
  people:
    '<svg width="12" height="12" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="6" cy="5" r="2.25"></circle><path d="M1.75 13.5c0-2.4 1.9-4 4.25-4s4.25 1.6 4.25 4"></path><path d="M10.5 2.9a2.25 2.25 0 0 1 0 4.3M12 9.8c1.4.5 2.25 1.9 2.25 3.7"></path></svg>',
  'folder-sm':
    '<svg width="12" height="12" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"><path d="M2 5a1 1 0 0 1 1-1h3l1.5 1.5H13a1 1 0 0 1 1 1V12a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1z"></path></svg>',
  'file-sm':
    '<svg width="12" height="12" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"><path d="M4 2h5l3.5 3.5V13a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V3a1 1 0 0 1 1-1z"></path><path d="M9 2v3.5h3.5"></path></svg>',
  'code-sm':
    '<svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M5.5 4.5L2 8l3.5 3.5M10.5 4.5L14 8l-3.5 3.5"></path></svg>',
  external:
    '<svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M9 3h4v4M13 3L7.5 8.5M11 9.5V12a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1h2.5"></path></svg>',
  'console-sm':
    '<svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M3 5l3 3-3 3M8 11.5h5"></path></svg>',
  clock:
    '<svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="8" cy="8" r="5.5"></circle><path d="M8 5v3l2 1.5"></path></svg>',
  lock: '<svg width="12" height="12" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><rect x="3.5" y="7" width="9" height="6.5" rx="1.5"></rect><path d="M5.5 7V5a2.5 2.5 0 0 1 5 0v2"></path></svg>',
  plus: '<svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"><path d="M8 3v10M3 8h10"></path></svg>',
  expand:
    '<svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="M2.5 6V2.5H6M10 2.5h3.5V6M13.5 10v3.5H10M6 13.5H2.5V10"></path></svg>',
  split:
    '<svg width="15" height="15" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.4"><rect x="2" y="3" width="12" height="10" rx="1.5"></rect><path d="M8 3v10"></path></svg>',
  dockLeft:
    '<svg width="15" height="15" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.4"><rect x="2" y="3" width="12" height="10" rx="1.5"></rect><rect x="2" y="3" width="4.5" height="10" rx="1.5" fill="currentColor"></rect></svg>',
  dockRight:
    '<svg width="15" height="15" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.4"><rect x="2" y="3" width="12" height="10" rx="1.5"></rect><rect x="9.5" y="3" width="4.5" height="10" rx="1.5" fill="currentColor"></rect></svg>',
  dockBottom:
    '<svg width="15" height="15" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.4"><rect x="2" y="3" width="12" height="10" rx="1.5"></rect><rect x="2" y="9" width="12" height="4" rx="1.5" fill="currentColor"></rect></svg>',
  check:
    '<svg width="12" height="12" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M3.5 8.5l3 3 6-6.5"></path></svg>',
  trash:
    '<svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"><path d="M2.5 4h11M6 4V2.5h4V4M4 4l.6 9h6.8L12 4M6.8 6.5v4M9.2 6.5v4"></path></svg>',
  warn: '<svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"><path d="M8 2l6.5 11.5h-13z"></path><path d="M8 6.5v3M8 11.5h.01"></path></svg>',
  close:
    '<svg width="12" height="12" viewBox="0 0 14 14" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"><path d="M3.5 3.5l7 7M10.5 3.5l-7 7"></path></svg>',
  info: '<svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="8" cy="8" r="6"></circle><path d="M8 7.5V11M8 5.2h.01"></path></svg>',
  help: '<svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="8" cy="8" r="6.25"></circle><path d="M6.3 6.3a1.8 1.8 0 1 1 2.6 1.6c-.6.3-.9.7-.9 1.3"></path><circle cx="8" cy="11.4" r=".5" fill="currentColor"></circle></svg>',
  back: '<svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M9.5 3.5L5 8l4.5 4.5"></path></svg>',
  bell: '<svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"><path d="M4 10.5h8l-1-1.5V6.5a3 3 0 0 0-6 0V9z"></path><path d="M6.8 12.5a1.3 1.3 0 0 0 2.4 0"></path></svg>',
  'bell-off':
    '<svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"><path d="M4 10.5h8l-1-1.5V6.5a3 3 0 0 0-6 0V9z"></path><path d="M6.8 12.5a1.3 1.3 0 0 0 2.4 0"></path><path d="M3 3l10 10"></path></svg>',
  gear: '<svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"><circle cx="8" cy="8" r="2"></circle><path d="M8 1.8v1.7M8 12.5v1.7M1.8 8h1.7M12.5 8h1.7M3.6 3.6l1.2 1.2M11.2 11.2l1.2 1.2M3.6 12.4l1.2-1.2M11.2 4.8l1.2-1.2"></path></svg>',
  spinner:
    '<svg class="fg-accent spinner" width="40" height="40" viewBox="0 0 40 40" aria-hidden="true"><circle cx="20" cy="20" r="15" fill="none" stroke="currentColor" stroke-opacity=".25" stroke-width="3"></circle><path d="M20 5a15 15 0 0 1 15 15" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round"></path></svg>',
  folder:
    '<svg class="empty-icon" width="40" height="40" viewBox="0 0 40 40" aria-hidden="true"><path d="M5 12h11l3 4h16v16a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2z" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round"></path></svg>',
  terminal:
    '<svg class="empty-icon" width="40" height="40" viewBox="0 0 40 40" aria-hidden="true"><rect x="4" y="7" width="32" height="26" rx="3" fill="none" stroke="currentColor" stroke-width="2"></rect><path d="M11 16l5 4-5 4M19 25h9" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"></path></svg>',
  // The icons an extension's view may name for its entry in the sidebar
  // (`argus_protocol::extensions::EXT_ICONS`), and the generic one.
  box: '<svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"><path d="M8 2l5.5 3v6L8 14l-5.5-3V5z"></path><path d="M2.5 5L8 8l5.5-3M8 8v6"></path></svg>',
  server:
    '<svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><rect x="2.5" y="2.5" width="11" height="4.5" rx="1"></rect><rect x="2.5" y="9" width="11" height="4.5" rx="1"></rect><path d="M5 4.75h.01M5 11.25h.01"></path></svg>',
  database:
    '<svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.5"><ellipse cx="8" cy="4" rx="5" ry="2"></ellipse><path d="M3 4v8c0 1.1 2.2 2 5 2s5-.9 5-2V4M3 8c0 1.1 2.2 2 5 2s5-.9 5-2"></path></svg>',
  globe:
    '<svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="8" cy="8" r="5.5"></circle><path d="M2.5 8h11M8 2.5c1.8 1.6 2.6 3.4 2.6 5.5S9.8 11.9 8 13.5C6.2 11.9 5.4 10.1 5.4 8S6.2 4.1 8 2.5z"></path></svg>',
  list: '<svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="M6 4h7.5M6 8h7.5M6 12h7.5M2.75 4h.01M2.75 8h.01M2.75 12h.01"></path></svg>',
  rocket:
    '<svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"><path d="M9.5 2.5c2 0 4 0 4 0s0 2-0 4L8.5 11.5l-4-4z"></path><path d="M4.5 7.5L2.5 8l2 1.5M8.5 11.5L8 13.5 6.5 11.5M10.5 5.5h.01"></path></svg>',
  console:
    '<svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M3 4.5l3.5 3.5L3 11.5M8 12h5"></path></svg>',
  keyboard:
    '<svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><rect x="1.5" y="4" width="13" height="8" rx="1.5"></rect><path d="M4 7h.01M6.7 7h.01M9.3 7h.01M12 7h.01M5 9.8h6"></path></svg>',
  code: '<svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M5.5 4.5L2 8l3.5 3.5M10.5 4.5L14 8l-3.5 3.5"></path></svg>',
  workspace:
    '<svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="3" width="12" height="10" rx="2"></rect><path d="M5 7l2 1.5L5 10M8.5 10.5H11"></path></svg>',
  puzzle:
    '<svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"><path d="M3 5h2.5a1.5 1.5 0 1 1 3 0H11v2.5a1.5 1.5 0 1 1 0 3V13H3z"></path></svg>',
  // The empty moments of the stages: a settled column, a folded one, a
  // search with nothing under it, a file not picked yet.
  settled:
    '<svg class="empty-icon" width="40" height="40" viewBox="0 0 40 40" aria-hidden="true"><circle cx="20" cy="20" r="14" fill="none" stroke="currentColor" stroke-width="2"></circle><path d="M13.5 20.5l4.5 4.5 8.5-9.5" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"></path></svg>',
  archive:
    '<svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="3" width="12" height="3" rx="1"></rect><path d="M3 6v6a1 1 0 0 0 1 1h8a1 1 0 0 0 1-1V6M6.5 9h3"></path></svg>',
  file: '<svg class="empty-icon" width="40" height="40" viewBox="0 0 40 40" aria-hidden="true"><path d="M10 5h13l8 8v21a1 1 0 0 1-1 1H10a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1z" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round"></path><path d="M23 5v8h8M14 21h12M14 27h8" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"></path></svg>',
  // The history's fold-all button, the sessions' filter, and the four verbs
  // a transcript's tool lines are told by.
  'fold-all':
    '<svg width="12" height="12" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M4 6l4-3 4 3M4 10l4 3 4-3"></path></svg>',
  filter:
    '<svg width="13" height="13" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="M2.5 4h11M4.5 8h7M6.5 12h3"></path></svg>',
  'tool-read':
    '<svg width="13" height="13" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linejoin="round"><path d="M3.5 1.5h6l3 3v10h-9z"></path></svg>',
  'tool-search':
    '<svg width="13" height="13" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><circle cx="7" cy="7" r="4.5"></circle><path d="M10.5 10.5L14 14"></path></svg>',
  'tool-edit':
    '<svg width="13" height="13" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"><path d="M11 2.5l2.5 2.5L6 12.5H3.5V10z"></path></svg>',
  'tool-run':
    '<svg width="13" height="13" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M3 4.5l3.5 3.5L3 11.5M8 12h5"></path></svg>',
} as const;

export type IconName = keyof typeof ICON;

/** What an element may hold: nodes, text, and nothing at all. */
export type Child = Node | string | number | null | undefined | false | readonly Child[];

/** Attributes, `class`, `on…` listeners; a null or false one is left off. */
export type Props = Record<string, unknown> | null;

export function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  props: Props,
  ...children: Child[]
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  for (const [key, value] of Object.entries(props ?? {})) {
    if (value == null || value === false) continue;
    if (key === 'class') node.className = String(value);
    // Through the style object, not the attribute: the window's CSP is
    // `style-src 'self'`, which drops a style attribute and allows this.
    else if (key === 'style') node.style.cssText = String(value);
    else if (key.startsWith('on') && typeof value === 'function') {
      node.addEventListener(key.slice(2), value as EventListener);
    } else node.setAttribute(key, value === true ? '' : String(value));
  }
  append(node, children);
  return node;
}

function append(node: Node, children: readonly Child[]): void {
  for (const child of children) {
    if (child == null || child === false) continue;
    if (Array.isArray(child)) append(node, child);
    else if (child instanceof Node) node.appendChild(child);
    else node.appendChild(document.createTextNode(String(child)));
  }
}

/** Grey bars standing where content will be, one per `[width, indent]`;
 *  a width under 1 is a fraction of the row. */
export function ghosts(bars: [number, number?][]): HTMLElement {
  return el(
    'div',
    { class: 'skeleton', 'aria-hidden': 'true' },
    bars.map(([width, indent]) => {
      const bar = el('div', { class: 'ghost' });
      bar.style.width = width < 1 ? `${width * 100}%` : `${width}px`;
      if (indent) bar.style.marginLeft = `${indent}px`;
      return bar;
    }),
  );
}

/** A label in the interface's one case: the keymap writes them lowercase. */
export function sentence(label: string): string {
  return label.charAt(0).toUpperCase() + label.slice(1);
}

export function icon(name: IconName): Element {
  const holder = document.createElement('span');
  holder.innerHTML = ICON[name];
  const svg = holder.firstElementChild;
  if (!svg) throw new Error(`the ${name} icon is empty`);
  return svg;
}

/** The ✕ a dialog's head ends with: the one way out that is always drawn. */
export function closeButton(onclick: () => void, label = 'Close'): HTMLElement {
  return el(
    'button',
    { class: 'icon', type: 'button', 'aria-label': label, onclick },
    icon('close'),
  );
}

/** The Esc keycap a dialog's safe button carries, so the footer says which
 *  button the key answers. */
export function escKey(): HTMLElement {
  return el('span', { class: 'kbd' }, 'Esc');
}

/** Gives a dialog's safe button the keys without drawing the focus ring:
 *  a dialog that opens already ringed reads as an error state. Tab still
 *  shows the ring, since it moves focus by keyboard. */
export function focusQuietly(node: HTMLElement | null | undefined): void {
  node?.focus({ focusVisible: false } as FocusOptions);
}

const STATUS_WORD: Record<string, string> = {
  waiting: 'needs you',
  'needs-review': 'review',
};

/** How a status is said, everywhere a status is said (the rail, the pill). */
export function statusWord(status: string): string {
  return STATUS_WORD[status] ?? status;
}

/** The one status pill: a dot and a word, tinted by how loud the status is
 *  (`.spill` in the stylesheet). The word is the daemon's, said as the rail
 *  says it, unless the caller has a better word for the same tone. */
export function statusPill(status: string, say = statusWord(status)): HTMLElement {
  return el('span', { class: `spill ${status}` }, say);
}

/** A key hint as every footer sets it: the keys, then what they do, with
 *  the gap between hints doing the separating. */
export function hint(keys: string | string[], what: string): HTMLElement {
  return el(
    'span',
    { class: 'kbar-key' },
    [keys].flat().map((key) => el('span', { class: 'kbd' }, key)),
    what,
  );
}

/** A feature's standing in the status pill's vocabulary, so a feature reads
 *  like an agent wherever it is named: working on it, held, done, or idle. */
export function standingPill(standing: string): HTMLElement {
  const status: Record<string, string> = { active: 'working', done: 'done', held: 'waiting' };
  return statusPill(status[standing] ?? 'idle', standing);
}
