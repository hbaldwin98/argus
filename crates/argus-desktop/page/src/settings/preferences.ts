// The Settings dialog, and the look its choices give the window: the theme
// on the document, the background's opacity as `--see` and the fonts in
// front of the bundled faces of `--font-mono` and `--font-ui`.
//
// Rust holds the choices (`src/settings/preferences.rs`) and saves each as
// it is made; the dialog draws what Rust sends back, and is updated in place
// rather than rebuilt, so a slider being dragged or a command being typed
// keeps its hold while the choices come back.

import type { Backdrop } from '../bindings/Backdrop';
import type { EditorMode } from '../bindings/EditorMode';
import type { Layout } from '../bindings/Layout';
import type { SavedLinks } from '../bindings/SavedLinks';
import type { SettingChange } from '../bindings/SettingChange';
import type { Theme } from '../bindings/Theme';
import { $, el, focusQuietly, icon, type Child, type IconName } from '../kit/dom.js';
import { fontPicker, type FontPicker } from './font_picker.js';
import { actionFor } from '../keys/keymap.js';
import { screens } from '../panes/screen.js';
import { state } from '../state.js';
import { call } from '../connection/tauri.js';

type Flavour = Exclude<Theme, 'system'>;

type Faces = { ui: string; mono: string };

// What the pickers say an empty field draws before the cards, which hold the
// per-theme answer, are built. Keep it with style.css.
const GEIST: Faces = { ui: 'Geist', mono: 'JetBrains Mono' };

/** The flavours' cards. Their colours and default fonts are style.css's:
 *  the preview carries the theme, so the stylesheet draws it. */
const FLAVOURS: { theme: Flavour; name: string }[] = [
  {
    theme: 'night',
    name: 'Argus Night',
  },
  {
    theme: 'ledger',
    name: 'Ledger',
  },
  {
    theme: 'carbon',
    name: 'Carbon',
  },
  {
    theme: 'flexoki',
    name: 'Flexoki',
  },
  {
    theme: 'kanagawa',
    name: 'Kanagawa',
  },
  {
    theme: 'rosePine',
    name: 'Rosé Pine',
  },
  {
    theme: 'nord',
    name: 'Nord',
  },
  {
    theme: 'radix',
    name: 'Radix',
  },
  {
    theme: 'mocha',
    name: 'Mocha',
  },
  {
    theme: 'macchiato',
    name: 'Macchiato',
  },
  {
    theme: 'frappe',
    name: 'Frappé',
  },
  {
    theme: 'latte',
    name: 'Latte',
  },
];

const BACKDROPS: Record<Backdrop, string> = {
  clear: 'Clear',
  mica: 'Mica',
  acrylic: 'Acrylic',
};

const EDITORS: [EditorMode, string][] = [
  ['floating', 'Floating'],
  ['checkout', 'In the checkout'],
  ['separately', 'Separately'],
];

const LAYOUTS: [Layout, string][] = [
  ['list', 'List'],
  ['board', 'Board'],
];

const LINKS: [SavedLinks, string][] = [
  ['dialog', 'Dialog'],
  ['column', 'Column'],
];

const SECTIONS = [
  'Appearance',
  'Editor',
  'Notifications',
  'Features',
  'Views',
  'Connection',
] as const;

type Section = (typeof SECTIONS)[number];

const SECTION_ICON: Record<Section, IconName> = {
  Appearance: 'workspace',
  Editor: 'code',
  Notifications: 'bell',
  Features: 'list',
  Views: 'split',
  Connection: 'server',
};

const SECTION_ABOUT: Record<Section, string> = {
  Appearance: 'The theme, the background and the fonts',
  Editor: 'Where a file opens, and with what',
  Notifications: 'When Argus tells you something needs you',
  Features: 'How the feature index opens',
  Views: 'Where the links in a saved view open',
  Connection: 'The daemon this window reaches',
};

const dark = matchMedia('(prefers-color-scheme: dark)');

function set(change: SettingChange): void {
  call('setting', { change });
}

/** The flavour drawn: the one chosen, or the system's. */
function drawn(theme: Theme): Flavour {
  if (theme !== 'system') return theme;
  return dark.matches ? 'mocha' : 'latte';
}

/** The bundled faces the window draws when no family is chosen. */
function defaultFaces(theme: Theme, cards: Map<Flavour, HTMLButtonElement>): Faces {
  const preview = cards.get(drawn(theme))?.querySelector('.pv');
  if (!preview) return GEIST;
  // Read off the card's preview, which the stylesheet scopes to its theme.
  const first = (property: string): string => {
    const stack = getComputedStyle(preview).getPropertyValue(property);
    return (stack.split(',')[0] ?? '').trim().replace(/^["']|["']$/g, '');
  };
  return {
    ui: first('--font-ui-bundled') || GEIST.ui,
    mono: first('--font-mono-bundled') || GEIST.mono,
  };
}

/** Puts the chosen theme and opacity on the window. The theme is left off
 *  the document when following the system, so the stylesheet's media
 *  query decides. */
export function applyLook(): void {
  const chosen = state.settings?.chosen;
  if (!chosen) return;
  const root = document.documentElement;
  if (chosen.theme === 'system') delete root.dataset.theme;
  else root.dataset.theme = chosen.theme;
  root.style.setProperty('--see', String(chosen.opacity / 100));
  // Rust leaves no quote in the name to end it early.
  putFont(root, '--font-mono', chosen.font, 'var(--font-mono-bundled)');
  putFont(root, '--font-ui', chosen.uiFont, 'var(--font-ui-bundled)');
  for (const s of screens) s.retheme();
}

/** A chosen family in front of the faces that draw what it lacks. Rust
 *  leaves no quote in the name to end it early. */
function putFont(root: HTMLElement, property: string, family: string, behind: string): void {
  if (family) root.style.setProperty(property, `"${family}", ${behind}`);
  else root.style.removeProperty(property);
}

/** The dialog's controls, kept to be updated in place. */
let controls: {
  cards: Map<Flavour, HTMLButtonElement>;
  follow: HTMLButtonElement;
  opacity: HTMLInputElement;
  percent: HTMLElement;
  backdrops: Map<Backdrop, HTMLButtonElement>;
  backdropRow: HTMLElement;
  backdropWhy: HTMLElement;
  font: FontPicker;
  fontWhy: HTMLElement;
  uiFont: FontPicker;
  uiFontWhy: HTMLElement;
  editors: Map<EditorMode, HTMLButtonElement>;
  command: HTMLInputElement;
  vim: HTMLButtonElement;
  grammars: HTMLButtonElement;
  notify: HTMLButtonElement;
  background: HTMLButtonElement;
  layouts: Map<Layout, HTMLButtonElement>;
  links: Map<SavedLinks, HTMLButtonElement>;
} | null = null;

/** What had the keys before the dialog, to give them back to. */
let before: Element | null = null;

export function settingsOpen(): boolean {
  return $('over').querySelector('.settings') !== null;
}

function toggle(label: string, onclick: () => void): HTMLButtonElement {
  return el(
    'button',
    { class: 'toggle', role: 'switch', 'aria-label': label, onclick },
    el('span', null),
  );
}

function setToggle(node: HTMLButtonElement, on: boolean): void {
  node.classList.toggle('on', on);
  node.setAttribute('aria-checked', String(on));
}

function segment<T extends string>(
  label: string,
  options: [T, string][],
  choose: (value: T) => void,
): [HTMLElement, Map<T, HTMLButtonElement>] {
  const buttons = new Map<T, HTMLButtonElement>();
  const group = el(
    'div',
    { class: 'seg', role: 'radiogroup', 'aria-label': label },
    options.map(([value, name]) => {
      const button = el(
        'button',
        { class: 'segb', role: 'radio', onclick: () => choose(value) },
        name,
      );
      buttons.set(value, button);
      return button;
    }),
  );
  return [group, buttons];
}

function setSegment<T extends string>(buttons: Map<T, HTMLButtonElement>, chosen: T): void {
  for (const [value, button] of buttons) {
    button.classList.toggle('on', value === chosen);
    button.setAttribute('aria-checked', String(value === chosen));
  }
}

function row(title: string, detail: string | null, control: HTMLElement): HTMLElement {
  return el(
    'div',
    { class: 'setrow' },
    el(
      'div',
      { class: 'grow stack-2' },
      el('span', null, title),
      detail && el('span', { class: 'meta' }, detail),
    ),
    control,
  );
}

/** A section: its title and one line under it, then its rows in one bordered
 *  group, divided from each other. */
function section(name: Section, ...rows: Child[]): HTMLElement {
  return el(
    'section',
    { class: 'setsection', 'aria-label': name, id: `settings-${name.toLowerCase()}` },
    el(
      'div',
      { class: 'sethead' },
      el('div', { class: 'settitle' }, name),
      el('div', { class: 'meta' }, SECTION_ABOUT[name]),
    ),
    el('div', { class: 'setgroup' }, rows),
  );
}

export function openSettings(): void {
  const page = state.settings;
  if (settingsOpen() || !page) return;
  before = document.activeElement;

  const cards = new Map<Flavour, HTMLButtonElement>();
  for (const flavour of FLAVOURS) {
    const preview = el(
      'span',
      { class: 'pv', 'data-theme': flavour.theme },
      el('span', { class: 'side' }),
      el('span', { class: 'lines' }, el('span', null), el('span', null)),
    );
    const card = el(
      'button',
      {
        class: 'themecard',
        role: 'radio',
        onclick: () => set({ set: 'theme', theme: flavour.theme }),
      },
      preview,
      el('span', { class: 'strong name' }, flavour.name, icon('check')),
    );
    cards.set(flavour.theme, card);
  }
  const follow = toggle('Follow the system', () => {
    const theme = state.settings?.chosen.theme ?? 'system';
    // Leaving the system keeps the flavour it was drawing.
    set({ set: 'theme', theme: theme === 'system' ? drawn('system') : 'system' });
  });

  const opacity = el('input', {
    type: 'range',
    min: String(page.minOpacity),
    max: '100',
    step: '1',
    'aria-label': 'Background opacity',
  });
  opacity.addEventListener('input', () => {
    set({ set: 'opacity', percent: Number(opacity.value) });
  });
  const percent = el('span', { class: 'n' });

  const [backdropGroup, backdrops] = segment(
    'What shows through',
    page.backdrops.map((offer): [Backdrop, string] => [offer.backdrop, BACKDROPS[offer.backdrop]]),
    (backdrop) => set({ set: 'backdrop', backdrop }),
  );
  const backdropWhy = el('span', { class: 'meta' });
  const backdropRow = el(
    'div',
    { class: 'setrow' },
    el(
      'div',
      { class: 'grow stack-2' },
      el('span', null, 'What shows through'),
      el('span', { class: 'meta' }, 'Clear shows the desktop; Mica and Acrylic blur it'),
      backdropWhy,
    ),
    backdropGroup,
  );

  const font = fontPicker('Code font', 'var(--font-mono-bundled)', GEIST.mono, (family) =>
    set({ set: 'font', family }),
  );
  const fontWhy = el('span', { class: 'meta' });
  const fontRow = el(
    'div',
    { class: 'setrow' },
    el(
      'div',
      { class: 'grow stack-2' },
      el('span', null, 'Code font'),
      el('span', { class: 'meta' }, 'Panes and code, monospaced; a Nerd Font draws its icons'),
      fontWhy,
    ),
    font.node,
  );
  const uiFont = fontPicker('Interface font', 'var(--font-ui-bundled)', GEIST.ui, (family) =>
    set({ set: 'uiFont', family }),
  );
  const uiFontWhy = el('span', { class: 'meta' });
  const uiFontRow = el(
    'div',
    { class: 'setrow' },
    el(
      'div',
      { class: 'grow stack-2' },
      el('span', null, 'Interface font'),
      el('span', { class: 'meta' }, 'Menus, lists, headings and prose'),
      uiFontWhy,
    ),
    uiFont.node,
  );

  const [editorGroup, editors] = segment('Files open', EDITORS, (mode) =>
    set({ set: 'editor', mode }),
  );
  const command = el('input', {
    class: 'field mono',
    placeholder: 'nvim',
    'aria-label': 'Editor command',
    autocomplete: 'off',
    spellcheck: 'false',
  });
  command.addEventListener('input', () => set({ set: 'editorCommand', command: command.value }));
  const vim = toggle("Vim's keys in the Code stage", () =>
    set({ set: 'vim', on: !(state.settings?.chosen.vim ?? true) }),
  );
  const grammars = toggle('Install known grammars without asking', () =>
    set({ set: 'installGrammars', on: !(state.settings?.chosen.installGrammars ?? false) }),
  );

  const notify = toggle('Notify when an agent needs you', () =>
    set({ set: 'notify', on: !(state.settings?.chosen.notify ?? true) }),
  );
  const background = toggle('Only while this window is in the background', () =>
    set({ set: 'notifyInBackground', only: !(state.settings?.chosen.notifyInBackground ?? true) }),
  );
  const [layoutGroup, layouts] = segment('Feature index layout', LAYOUTS, (layout) =>
    set({ set: 'featureLayout', layout }),
  );
  const [linksGroup, links] = segment('Links in a saved view open', LINKS, (opens) =>
    set({ set: 'savedLinks', opens }),
  );

  controls = {
    cards,
    follow,
    opacity,
    percent,
    backdrops,
    backdropRow,
    backdropWhy,
    font,
    fontWhy,
    uiFont,
    uiFontWhy,
    editors,
    command,
    vim,
    grammars,
    notify,
    background,
    layouts,
    links,
  };

  const body = el(
    'div',
    { class: 'settings-body' },
    section(
      'Appearance',
      el('div', { class: 'themecards', role: 'radiogroup', 'aria-label': 'Theme' }, [
        ...cards.values(),
      ]),
      row('Follow the system', 'Mocha when it is dark, Latte when it is light', follow),
      row(
        'Background opacity',
        'The desktop shows through the background; text and borders stay solid',
        el('div', { class: 'opacity' }, opacity, percent),
      ),
      // Clear alone is no choice: where it is all this system draws, the
      // row is left out.
      page.backdrops.length > 1 ? backdropRow : null,
      fontRow,
      uiFontRow,
    ),
    section(
      'Editor',
      row('Files open', 'A known GUI editor always opens on its own', editorGroup),
      row('Editor command', 'Empty uses $VISUAL, then $EDITOR', command),
      row('Vim keys', 'In the Code stage: normal, insert and visual modes, and :w', vim),
      row(
        'Install grammars without asking',
        'Colours for a language Argus knows download from npm when its file opens; one found by searching npm is still asked about',
        grammars,
      ),
    ),
    section(
      'Notifications',
      row(
        'When an agent needs you',
        'A system notification naming the pane: waiting, needs-review or failed',
        notify,
      ),
      row('Only while this window is in the background', null, background),
    ),
    section(
      'Features',
      row(
        'Feature index',
        'Remembered per repository; this is what a new one opens as',
        layoutGroup,
      ),
    ),
    section(
      'Views',
      row(
        'Links in a saved view open',
        'A dialog over the stage, or the next column beside the view',
        linksGroup,
      ),
    ),
    section('Connection', el('div', { class: 'meta placeholder' }, 'Nothing to set here yet.')),
  );

  const nav = el(
    'nav',
    { class: 'settings-nav', 'aria-label': 'Settings sections' },
    SECTIONS.map((name) =>
      el(
        'button',
        {
          class: 'frow',
          onclick: () => {
            document.getElementById(`settings-${name.toLowerCase()}`)?.scrollIntoView({
              block: 'start',
            });
          },
        },
        icon(SECTION_ICON[name]),
        name,
      ),
    ),
    el('div', { class: 'grow' }),
    page.version && el('div', { class: 'meta setversion' }, `argus ${page.version}`),
  );

  // The section in view is the one lit in the nav, so a long page says where it is.
  const spy = (): void => {
    const top = body.getBoundingClientRect().top;
    let current: string = SECTIONS[0];
    for (const name of SECTIONS) {
      const block = document.getElementById(`settings-${name.toLowerCase()}`);
      if (block && block.getBoundingClientRect().top - top <= 24) current = name;
    }
    nav.querySelectorAll('button').forEach((button) => {
      button.classList.toggle('on', button.textContent === current);
    });
  };
  body.addEventListener('scroll', spy);
  spy();

  const close = el(
    'button',
    { class: 'icon', 'aria-label': 'Close settings', onclick: closeSettings },
    icon('close'),
  );
  const dialog = el(
    'section',
    {
      class: 'dialog sheet-dialog settings',
      role: 'dialog',
      'aria-modal': 'true',
      'aria-labelledby': 'settings-title',
    },
    el(
      'div',
      { class: 'dhead' },
      el('div', { class: 'type-title grow', id: 'settings-title' }, 'Settings'),
      el('span', { class: 'meta saved' }, icon('check'), 'Saved as you change them'),
      close,
    ),
    el('div', { class: 'split' }, nav, body),
  );
  // Escape puts it away from any of its controls, a field included.
  dialog.addEventListener('keydown', (e) => {
    const action = actionFor(['anywhere'], e);
    if (action === 'closeLayer' || action === 'settings') {
      e.preventDefault();
      e.stopPropagation();
      closeSettings();
    }
  });
  $('over').replaceChildren(
    el(
      'div',
      {
        class: 'scrim',
        onmousedown: (e: MouseEvent) => {
          if (e.target === e.currentTarget) closeSettings();
        },
      },
      dialog,
    ),
  );
  drawSettings();
  $('input').blur();
  focusQuietly(close);
}

export function closeSettings(): void {
  if (!settingsOpen()) return;
  $('over').replaceChildren();
  controls = null;
  if (before instanceof HTMLElement) before.focus();
  before = null;
}

const probe = document.createElement('canvas').getContext('2d');
const PROBE_TEXT = 'mmmmmmmmmmlli0O@#';
const drawable = new Map<string, boolean>();

/** Whether this webview finds a font by that name. A name it does not know
 *  draws in the fallback after it, so text measures as the fallback alone
 *  does; one it knows measures unlike at least one of three generics.
 *  Rust offers both of a font's names, and a webview knows it by only one. */
function draws(family: string): boolean {
  let found = drawable.get(family);
  if (found !== undefined) return found;
  found = false;
  for (const generic of ['monospace', 'serif', 'sans-serif']) {
    if (!probe) break;
    probe.font = `32px ${generic}`;
    const fallback = probe.measureText(PROBE_TEXT).width;
    probe.font = `32px "${family}", ${generic}`;
    if (probe.measureText(PROBE_TEXT).width !== fallback) {
      found = true;
      break;
    }
  }
  drawable.set(family, found);
  return found;
}

/** Says under a font field when nothing is called what it holds. */
function missingNote(note: HTMLElement, family: string, instead: string): void {
  const missing = family !== '' && !draws(family);
  note.textContent = missing ? `No font here is called that, so ${instead} draws` : '';
  note.hidden = !missing;
}

/** The choices Rust sent: the window's look always, and the dialog's
 *  controls while it is open. */
export function drawSettings(): void {
  applyLook();
  const page = state.settings;
  if (!controls || !page || !settingsOpen()) return;
  const { chosen } = page;
  const shown = drawn(chosen.theme);
  const faces = defaultFaces(chosen.theme, controls.cards);
  for (const [theme, card] of controls.cards) {
    card.classList.toggle('on', theme === shown);
    card.setAttribute('aria-checked', String(theme === shown));
  }
  setToggle(controls.follow, chosen.theme === 'system');
  // A thumb being dragged is where the person has it already.
  if (document.activeElement !== controls.opacity) controls.opacity.value = String(chosen.opacity);
  controls.percent.textContent = `${chosen.opacity}%`;
  setSegment(controls.backdrops, chosen.backdrop);
  const why: string[] = [];
  for (const offer of page.backdrops) {
    const button = controls.backdrops.get(offer.backdrop);
    if (!button) continue;
    button.disabled = offer.unavailable !== null;
    button.title = offer.unavailable ?? '';
    if (offer.unavailable) why.push(offer.unavailable);
  }
  controls.backdropWhy.textContent = why.join('. ');
  controls.backdropWhy.hidden = why.length === 0;
  const installed = page.fonts.filter((f) => draws(f.name));
  controls.font.defaultName(faces.mono);
  controls.uiFont.defaultName(faces.ui);
  controls.font.update(
    installed.filter((f) => f.mono).map((f) => f.name),
    chosen.font,
  );
  controls.uiFont.update(
    installed.map((f) => f.name),
    chosen.uiFont,
  );
  missingNote(controls.fontWhy, chosen.font, faces.mono);
  missingNote(controls.uiFontWhy, chosen.uiFont, faces.ui);
  setSegment(controls.editors, chosen.editor);
  // A field being typed in is ahead of what came back.
  if (document.activeElement !== controls.command) controls.command.value = chosen.editorCommand;
  setToggle(controls.vim, chosen.vim);
  setToggle(controls.grammars, chosen.installGrammars);
  setToggle(controls.notify, chosen.notify);
  setToggle(controls.background, chosen.notifyInBackground);
  controls.background.disabled = !chosen.notify;
  setSegment(controls.layouts, chosen.featureLayout);
  setSegment(controls.links, chosen.savedLinks);
}

// Following the system, a change of its light or dark changes the cards.
dark.addEventListener('change', () => drawSettings());
