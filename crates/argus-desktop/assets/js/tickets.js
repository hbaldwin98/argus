// A feature's link to a ticket in the team's tracker, and the view of it
// an agent showed, when one waits: the link then opens the view, which says
// more than the tracker's page at a glance, and the tracker is a click away
// from there. Without one it opens the ticket in the browser.
import { el } from './dom.js';
import { call } from './tauri.js';
import { viewOf } from './views.js';
export function ticketLink(link) {
    const view = viewOf(link);
    return el('button', {
        class: view ? 'track viewed' : 'track',
        type: 'button',
        title: view ? `${view.view.title}, as ${view.from || 'an agent'} read it` : link.url,
        onclick: (event) => {
            event.stopPropagation();
            if (view)
                call('open_view', { pane: view.pane, id: view.view.id });
            else
                call('open_url', { url: link.url });
        },
    }, link.tracker, ' ', el('b', null, link.key), view && el('span', { class: 'vdot', 'aria-label': 'a view of it waits' }));
}
export function ticketLinks(links) {
    return links.length > 0 && el('span', { class: 'actions tickets' }, links.map(ticketLink));
}
