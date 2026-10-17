// A label column beside a value column: the shape of a ticket's details.

import type { Field } from '../../bindings/Field';
import { el } from '../../kit/dom.js';
import { value } from '../values.js';
import type { ViewCtx } from '../ctx.js';

export function fields(list: readonly Field[], ctx: ViewCtx | null = null): HTMLElement {
  return el(
    'dl',
    { class: 'vfields' },
    list.flatMap((f) => [el('dt', null, f.label), el('dd', null, value(f.value, ctx))]),
  );
}
