// A code block with its language and a Copy button in a header. Plain
// text: no highlighter here.

import { copyButton } from '../../system/copy.js';
import { el } from '../../kit/dom.js';

export function code(lang: string | null, text: string): HTMLElement {
  return el(
    'div',
    { class: 'vcode' },
    el(
      'div',
      { class: 'vcode-hd' },
      el('span', { class: 'vcode-lang' }, lang),
      copyButton('Copy', () => text),
    ),
    el('pre', { class: 'vcode-bd' }, text),
  );
}
