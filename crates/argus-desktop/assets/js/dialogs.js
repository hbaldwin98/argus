// The question an extension puts to this window — a confirm, a one-line
// prompt, a pick from a list or a small form — and the answer or the
// cancellation it sends back. Rust holds the questions
// (`src/extension_dialog.rs`) and which is open; the page draws that one.
//
// One that answers a command the person just ran takes the keys. One raised
// on its own is drawn without a scrim and waits for a click, so a pane being
// typed in keeps every key.
import { $, el } from './kit/dom.js';
import { actionFor } from './keys/keymap.js';
import { call } from './connection/tauri.js';
/** The dialog drawn, so a page that only changed the count behind it
 *  keeps what is typed. */
let drawn = null;
function answer(id, given) {
    call('answer_dialog', { dialog: id, answer: given });
}
/** Whether the question has the keys. */
export function extensionDialogFocused() {
    return $('ext-dialog').contains(document.activeElement);
}
export function drawExtensionDialog(page) {
    const holder = $('ext-dialog');
    const dialog = page.dialog;
    if (!page.open || !dialog) {
        drawn = null;
        holder.replaceChildren();
        return;
    }
    if (drawn === page.id) {
        const waiting = holder.querySelector('.ext-waiting');
        if (waiting)
            waiting.textContent = waitingWords(page.waiting);
        return;
    }
    drawn = page.id;
    const id = page.id;
    const cancel = () => answer(id, null);
    let body;
    let submit;
    const kind = dialog.kind;
    if ('confirm' in kind) {
        body = el('div', { class: 'fg-muted' }, kind.confirm.message);
        submit = () => answer(id, 'yes');
    }
    else if ('prompt' in kind) {
        const field = el('input', {
            class: 'field',
            id: 'ext-dialog-first',
            'aria-label': kind.prompt.message,
        });
        field.value = kind.prompt.initial;
        body = [el('div', { class: 'fg-muted' }, kind.prompt.message), field];
        submit = () => answer(id, { text: field.value });
    }
    else if ('pick' in kind) {
        const list = pickList(kind.pick.items, (key) => answer(id, { picked: key }));
        body = [el('div', { class: 'fg-muted' }, kind.pick.message), list.node];
        submit = list.choose;
    }
    else {
        const fields = kind.form.fields.map((f, i) => {
            const input = el('input', {
                class: 'field',
                id: i === 0 ? 'ext-dialog-first' : null,
                'aria-label': f.label,
            });
            input.value = f.initial;
            return { name: f.name, label: f.label, input };
        });
        body = fields.map((f) => el('label', { class: 'label' }, f.label, f.input));
        submit = () => answer(id, { form: Object.fromEntries(fields.map((f) => [f.name, f.input.value])) });
    }
    const yes = 'confirm' in kind ? (kind.confirm.yes ?? 'Yes') : 'pick' in kind ? 'Choose' : 'OK';
    const card = el('section', {
        class: `dialog ext-dialog${page.prompted ? '' : ' unasked'}`,
        role: 'dialog',
        'aria-labelledby': 'ext-dialog-title',
        onkeydown: (e) => {
            const action = actionFor(['writing'], e);
            // Enter on a button is that button's click, Cancel's included.
            if (action === 'choose' && e.target instanceof HTMLButtonElement)
                return;
            if (action === 'closeLayer') {
                e.preventDefault();
                cancel();
            }
            else if (action === 'choose' || action === 'save') {
                e.preventDefault();
                submit();
            }
        },
    }, el('div', { class: 'dhead' }, el('div', { class: 'type-title', id: 'ext-dialog-title' }, dialog.title)), el('div', { class: 'dbody' }, body), el('div', { class: 'dfoot' }, el('span', { class: 'meta' }, `from ${page.extension}`), el('span', { class: 'meta ext-waiting' }, waitingWords(page.waiting)), el('span', { class: 'grow' }), el('button', { class: 'btn', type: 'button', onclick: cancel }, 'Cancel'), el('button', { class: 'btn primary', type: 'button', id: 'ext-dialog-yes', onclick: submit }, yes)));
    holder.replaceChildren(card);
    if (page.prompted) {
        (document.getElementById('ext-dialog-first') ?? document.getElementById('ext-dialog-yes'))?.focus();
    }
}
function waitingWords(waiting) {
    return waiting > 0 ? ` · ${waiting} more waiting` : '';
}
/** A list to pick one from, moved through with the palette's keys. */
function pickList(items, picked) {
    let at = 0;
    const buttons = items.map((item, i) => el('button', {
        class: `mi${i === 0 ? ' on' : ''}`,
        type: 'button',
        id: i === 0 ? 'ext-dialog-first' : null,
        onclick: () => picked(item.key),
    }, item.label));
    const move = (to) => {
        at = Math.max(0, Math.min(buttons.length - 1, to));
        buttons.forEach((b, i) => b.classList.toggle('on', i === at));
        buttons[at]?.focus();
    };
    const node = el('div', {
        class: 'menu ext-pick',
        role: 'listbox',
        onkeydown: (e) => {
            const action = actionFor(['palette'], e);
            if (action === 'next' || action === 'previous') {
                e.preventDefault();
                e.stopPropagation();
                move(at + (action === 'next' ? 1 : -1));
            }
        },
    }, buttons);
    return {
        node,
        choose: () => {
            const item = items[at];
            if (item)
                picked(item.key);
        },
    };
}
