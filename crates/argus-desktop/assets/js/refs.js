// The values that point into Argus rather than out of it — a path, a
// commit, a feature, a pane — and the plain ones drawn beside them: a
// duration and inline code.
import { el } from './dom.js';
/** Not drawn yet: the value's kind. */
export function refValue(v, pane) {
    void pane;
    return el('span', { class: 'vpending' }, v.kind);
}
