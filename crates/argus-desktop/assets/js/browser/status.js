// Who has a browser, said the same way on a tile, in a cell's tab strip and
// by the dot on a sidebar row.
import { el } from '../kit/dom.js';
/** Who has the browser, said only when someone does. */
export function status(browser) {
    if (browser.held)
        return el('span', { class: 'bstatus held' }, 'You have control');
    if (browser.driver === 'agent')
        return el('span', { class: 'bstatus agent' }, 'Agent driving');
    if (browser.driver === 'person')
        return el('span', { class: 'bstatus person' }, 'You’re driving');
    return null;
}
export function dotKind(browser) {
    if (browser.held)
        return ' held';
    if (browser.driver === 'agent')
        return ' agent';
    if (browser.driver === 'person')
        return ' person';
    return '';
}
