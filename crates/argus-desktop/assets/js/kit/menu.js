// Menus over the stage, and what a right-click opens. A menu draws in the
// layer at the pointer and holds the keys until it closes. A right-click
// on anything that offers no menu of its own opens nothing, so WebView2's
// (Back, Reload, Inspect) never shows over the window; a text field keeps
// it for cut and paste, and selected text gets a menu to copy it.
import { $, el } from './dom.js';
import { actionFor } from '../keys/keymap.js';
export function closeLayer() {
    $('layer').replaceChildren();
}
/** Puts a built menu in the layer at `at`, kept inside the window, and
 *  gives its first choosable item the keys. */
export function placeMenu(menu, at) {
    $('layer').replaceChildren(menu);
    const box = menu.getBoundingClientRect();
    // `end` hangs the menu from its right edge, under a button at the strip's end.
    const x = at.align === 'end' ? at.x - box.width : at.x;
    menu.style.left = `${Math.max(8, Math.min(x, window.innerWidth - box.width - 8))}px`;
    menu.style.top = `${Math.max(8, Math.min(at.y, window.innerHeight - box.height - 8))}px`;
    menu.querySelector('.mi:not([disabled])')?.focus();
}
export function openMenu(label, items, at) {
    const shown = items.filter((item) => !!item);
    const menu = el('div', { class: 'menu', role: 'menu', 'aria-label': label }, shown.map((item) => {
        if (item === 'sep')
            return el('div', { class: 'msep' });
        if ('heading' in item)
            return el('div', { class: 'mlabel ellipsis' }, item.heading);
        const hint = item.disabled || item.hint;
        return el('button', {
            class: `mi${item.danger ? ' danger' : ''}`,
            role: 'menuitem',
            disabled: !!item.disabled,
            onclick: () => {
                closeLayer();
                item.run();
            },
        }, el('span', { class: 'grow' }, item.label), typeof hint === 'string' ? el('span', { class: 'meta' }, hint) : hint || null);
    }));
    placeMenu(menu, at);
}
/** An `oncontextmenu` that opens the menu `build` makes, at the pointer. */
export function contextMenu(label, build) {
    return (e) => {
        e.preventDefault();
        e.stopPropagation();
        openMenu(label, build(), { x: e.clientX, y: e.clientY });
    };
}
function editable(target) {
    return (target instanceof Element &&
        !!target.closest('input, textarea, select, [contenteditable=""], [contenteditable="true"]'));
}
function copyMenu(text, at) {
    openMenu('Selection', [{ label: 'Copy', run: () => void navigator.clipboard.writeText(text) }], at);
}
export function listen() {
    // Bubbling, so a menu an element opened has already said so.
    document.addEventListener('contextmenu', (e) => {
        if (e.defaultPrevented || editable(e.target))
            return;
        e.preventDefault();
        const selected = window.getSelection()?.toString() ?? '';
        if (selected.trim())
            copyMenu(selected, { x: e.clientX, y: e.clientY });
    });
    document.addEventListener('mousedown', (e) => {
        const layer = $('layer');
        if (layer.querySelector('.menu') && e.target instanceof Node && !layer.contains(e.target)) {
            closeLayer();
        }
    });
    // Capturing, ahead of the stage's keys; Enter is the focused button's own.
    document.addEventListener('keydown', (e) => {
        const menu = $('layer').querySelector('.menu');
        const action = menu ? actionFor(['menu'], e) : null;
        if (!menu || (action !== 'next' && action !== 'previous'))
            return;
        const items = [...menu.querySelectorAll('.mi:not([disabled])')];
        if (!items.length)
            return;
        e.preventDefault();
        e.stopPropagation();
        const at = items.indexOf(document.activeElement);
        const step = action === 'next' ? 1 : -1;
        items[(at + step + items.length) % items.length]?.focus();
    }, true);
}
