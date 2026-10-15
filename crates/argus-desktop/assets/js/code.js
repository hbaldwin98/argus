// A code block with its language in a header. Plain text: the page cannot
// write the clipboard, so there is no copy button, and no highlighter here.
import { el } from './dom.js';
export function code(lang, text) {
    return el('div', { class: 'vcode' }, lang ? el('div', { class: 'vcode-hd' }, lang) : null, el('pre', { class: 'vcode-bd' }, text));
}
