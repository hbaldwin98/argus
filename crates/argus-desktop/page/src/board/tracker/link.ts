// The link dialog: a ticket's URL pasted and read as its tracker and key,
// what it is to the feature, the tickets already linked to open or unlink,
// and the repository's tracker map with its editor. Rust holds the dialog
// (`src/board/tracker/link_dialog.rs`) and reads the URL as the daemon
// will; the page draws it and says what was typed and pressed.

import type { Action } from '../../bindings/Action';
import type { LinkGesture } from '../../bindings/LinkGesture';
import type { LinkPage } from '../../bindings/LinkPage';
import { $, el, escKey, icon, type Child } from '../../kit/dom.js';
import { actionFor } from '../../keys/keymap.js';
import { call } from '../../connection/tauri.js';

let page: LinkPage | null = null;

function send(gesture: LinkGesture): void {
  call('link', { gesture });
}

/** A key of the dialog's own, outside its fields. */
export function linkAction(action: Action): void {
  send({ on: 'action', action });
}

function keycap(key: string): HTMLElement {
  return el('span', { class: 'kbd' }, key);
}

function valueOf(id: string): string {
  const node = document.getElementById(id);
  return node instanceof HTMLInputElement ? node.value : '';
}

function fields(): { url: string; role: string } {
  return { url: valueOf('link-url'), role: valueOf('link-role') };
}

/** A field whose text is its own once drawn, as the sessions search's is. */
function input(id: string, label: string, value: string, placeholder: string): HTMLElement {
  const live = document.getElementById(id);
  const field = el('input', {
    class: 'field',
    id,
    'aria-label': label,
    placeholder,
    spellcheck: 'false',
    oninput: () => send({ on: 'fields', ...fields() }),
    onkeydown: (e: KeyboardEvent) => {
      const action = actionFor(['writing'], e);
      if (action === 'choose') {
        e.preventDefault();
        send({ on: 'link', ...fields() });
      } else if (action === 'closeLayer') {
        e.preventDefault();
        send({ on: 'close' });
      }
    },
  });
  field.value = live instanceof HTMLInputElement ? live.value : value;
  return field;
}

function read(p: LinkPage): Child {
  if (p.read) {
    return el(
      'div',
      { class: 'meta' },
      el('span', { class: 'fg-ok' }, '✓ '),
      `${p.read.tracker} · key `,
      el('span', { class: 'mono fg' }, p.read.key),
      ' · read from the URL, without going online',
    );
  }
  if (p.refusal) return el('div', { class: 'meta fg-attention' }, p.refusal);
  return el(
    'div',
    { class: 'meta' },
    'Linear, Jira and GitHub URLs name their ticket; any other is its host and last path segment.',
  );
}

function linked(p: LinkPage): HTMLElement {
  return el(
    'section',
    { class: 'stack-6', 'aria-label': 'Already linked' },
    el('div', { class: 'label' }, 'Already linked'),
    p.linked.length === 0 && el('div', { class: 'meta' }, 'No ticket yet.'),
    p.linked.map((row, index) =>
      el(
        'div',
        {
          class: `hrow lrow${row.on ? ' on' : ''}`,
          onclick: () => send({ on: 'select', index }),
        },
        el('span', { class: 'meta tracker' }, row.link.tracker),
        el('span', { class: 'mono fg-ref' }, row.link.key),
        el('span', { class: 'meta grow ellipsis' }, row.link.role),
        el(
          'button',
          {
            class: 'btn sm',
            type: 'button',
            title: row.link.url,
            onclick: () => call('open_url', { url: row.link.url }),
          },
          'Open',
        ),
        el(
          'button',
          {
            class: 'btn sm',
            type: 'button',
            onclick: () => send({ on: 'unlink', key: row.link.key }),
          },
          'Unlink',
        ),
      ),
    ),
  );
}

function map(p: LinkPage): HTMLElement {
  const editing = p.editingMap;
  if (editing !== null) {
    const area = el('textarea', {
      class: 'field area tall',
      id: 'link-map',
      'aria-label': 'The tracker map',
      placeholder: 'Linear: a feature is an issue, a task a sub-issue…',
      oninput: (e: Event) => send({ on: 'mapText', text: (e.target as HTMLTextAreaElement).value }),
      onkeydown: (e: KeyboardEvent) => {
        const action = actionFor(['writing'], e);
        if (action === 'save') {
          e.preventDefault();
          send({ on: 'saveMap', text: (e.target as HTMLTextAreaElement).value });
        } else if (action === 'closeLayer') {
          e.preventDefault();
          send({ on: 'cancelMap' });
        }
      },
    });
    const live = document.getElementById('link-map');
    area.value = live instanceof HTMLTextAreaElement ? live.value : editing;
    const save = (): void => send({ on: 'saveMap', text: area.value });
    return el(
      'section',
      { class: 'stack-6 link-map' },
      el(
        'div',
        { class: 'label' },
        p.map.draft !== null
          ? `Tracker map · draft from ${p.map.draftBy ?? 'an agent'}`
          : 'Tracker map',
      ),
      area,
      el(
        'div',
        { class: 'actions' },
        el('span', { class: 'grow' }),
        el('button', { class: 'btn sm', onclick: () => send({ on: 'cancelMap' }) }, 'Cancel'),
        p.map.draft !== null &&
          el(
            'button',
            { class: 'btn sm', onclick: () => send({ on: 'discardDraft' }) },
            'Turn the draft down',
          ),
        el(
          'button',
          { class: 'btn sm primary', onclick: save },
          p.map.draft !== null ? 'Approve as written ' : 'Save the map ',
          el('span', { class: 'kbd on-accent' }, 'Ctrl+Enter'),
        ),
      ),
    );
  }
  const lines = p.map.body.split('\n').filter((line) => line.trim() !== '');
  return el(
    'section',
    { class: 'stack-6 link-map' },
    el(
      'div',
      { class: 'label' },
      'The repository’s tracker map',
      p.map.draft !== null &&
        el(
          'span',
          { class: 'pill fg-attention' },
          `draft from ${p.map.draftBy ?? 'an agent'} waits`,
        ),
    ),
    lines.length > 0
      ? el(
          'div',
          { class: 'meta map-lines' },
          lines.slice(0, 6).map((line) => el('div', { class: 'ellipsis' }, line)),
        )
      : el('div', { class: 'meta' }, 'No map yet: write one, or ask an agent to draft it.'),
    el(
      'div',
      { class: 'actions' },
      el('span', { class: 'meta grow' }, 'An agent reads the map whenever it links or reconciles.'),
      el(
        'button',
        { class: 'btn sm', onclick: () => send({ on: 'editMap' }) },
        'Edit the map… ',
        keycap('M'),
      ),
    ),
  );
}

export function drawLink(next: LinkPage): void {
  const opening = page?.open !== true;
  const editingBefore = page?.editingMap ?? null;
  page = next;
  const host = $('link');
  if (!next.open) {
    host.hidden = true;
    host.replaceChildren();
    return;
  }
  const focused = document.activeElement?.id ?? '';
  const had = focused.startsWith('link-') ? focused : null;
  host.hidden = false;
  const p = next;
  const key = p.read?.key;
  host.replaceChildren(
    el(
      'div',
      { class: 'scrim' },
      el(
        'section',
        {
          class: 'dialog tracker-dialog',
          role: 'dialog',
          'aria-modal': 'true',
          'aria-labelledby': 'link-title',
        },
        el(
          'div',
          { class: 'dhead' },
          el(
            'div',
            { class: 'stack-6 grow' },
            el('div', { class: 'type-title', id: 'link-title' }, `Link #${p.number} to a ticket`),
            el(
              'div',
              { class: 'meta' },
              'Argus keeps each ticket’s key and URL. Titles and statuses stay in the tracker, so nothing shown here can go stale.',
            ),
          ),
          el(
            'button',
            { class: 'icon', 'aria-label': 'Close', onclick: () => send({ on: 'close' }) },
            icon('close'),
          ),
        ),
        el(
          'div',
          { class: 'tracker-body stack-12' },
          el(
            'label',
            { class: 'stack-6' },
            el('span', { class: 'label' }, 'Ticket URL'),
            input('link-url', 'Ticket URL', p.url, 'https://linear.app/…/issue/ENG-88/…'),
            read(p),
          ),
          el(
            'label',
            { class: 'stack-6' },
            el('span', { class: 'label' }, 'What it is to this feature'),
            input('link-role', 'Role', p.role, 'the work, the PM’s ticket… (optional)'),
          ),
          linked(p),
          map(p),
        ),
        el(
          'div',
          { class: 'dfoot' },
          el(
            'span',
            {
              class: 'dwhy grow',
              title: 'J and K move through the tickets; O opens one, X unlinks it.',
            },
            icon('info'),
            'Nothing is written to a tracker by linking.',
          ),
          el('button', { class: 'btn', onclick: () => send({ on: 'close' }) }, 'Cancel ', escKey()),
          el(
            'button',
            {
              class: 'btn primary',
              disabled: !p.read || p.pending,
              onclick: () => send({ on: 'link', ...fields() }),
            },
            p.pending ? 'Linking…' : key ? `Link ${key} ` : 'Link ',
            el('span', { class: 'kbd on-accent' }, 'Enter'),
          ),
        ),
      ),
    ),
  );
  // Redrawn as each letter arrives: the field typed in keeps the keys and
  // the caret; a dialog newly open puts them in the URL, and the map's
  // editor newly open in the map. Otherwise the dialog keeps its own keys.
  let wanted = had;
  if (next.editingMap !== null && editingBefore === null) wanted = 'link-map';
  else if (opening) wanted = 'link-url';
  const again = wanted ? document.getElementById(wanted) : null;
  if (again instanceof HTMLInputElement || again instanceof HTMLTextAreaElement) {
    if (document.activeElement !== again) {
      again.focus();
      again.setSelectionRange(again.value.length, again.value.length);
    }
  }
}

export function linkOpen(): boolean {
  return page?.open === true;
}
