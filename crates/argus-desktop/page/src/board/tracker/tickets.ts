// A feature's link to a ticket in the team's tracker, and the view of it
// an agent showed, when one waits: the link then opens the view, which says
// more than the tracker's page at a glance, and the tracker is a click away
// from there. Without one it opens the ticket in the browser.

import type { FeatureViewLink } from '../../bindings/FeatureViewLink';
import type { Link } from '../../bindings/Link';
import type { PageSaved } from '../../bindings/PageSaved';
import { el } from '../../kit/dom.js';
import type { MenuItem } from '../../kit/menu.js';
import { forFeature } from '../../views/saved/logic.js';
import { state } from '../../state.js';
import { call } from '../../connection/tauri.js';
import { viewOf } from '../../views/agent.js';

export function ticketLink(link: Link): HTMLElement {
  const view = viewOf(link);
  return el(
    'button',
    {
      class: view ? 'track viewed' : 'track',
      type: 'button',
      title: view
        ? `${view.view.title}, as ${view.from || 'an agent'} read it`
        : link.role
          ? `${link.role}: ${link.url}`
          : link.url,
      onclick: (event: MouseEvent) => {
        event.stopPropagation();
        if (view) call('open_view', { pane: view.pane, id: view.view.id });
        else call('open_url', { url: link.url });
      },
    },
    link.tracker,
    ' ',
    el('b', null, link.key),
    view && el('span', { class: 'vdot', 'aria-label': 'a view of it waits' }),
  );
}

/** A view an agent showed that names the feature: it opens that view in the
 *  views' panel. */
export function featureViewItem(link: FeatureViewLink): MenuItem {
  return {
    label: link.title,
    hint: 'agent view',
    run: () => call('open_view', { pane: link.pane, id: link.id }),
  };
}

/** The saved views scoped or attached to feature `number` of the chosen
 *  repository, each opening its panel. */
export function savedFeatureItems(number: number): MenuItem[] {
  return forFeature(state.saved?.views ?? [], state.rail.repository, number).map(savedFeatureItem);
}

function savedFeatureItem(view: PageSaved): MenuItem {
  return {
    label: view.view.title,
    hint: view.stale ? 'saved, out of date' : 'saved view',
    run: () => call('open_saved_view', { id: view.id, from: null }),
  };
}

export function ticketLinks(links: readonly Link[]): HTMLElement | false {
  return links.length > 0 && el('span', { class: 'actions tickets' }, links.map(ticketLink));
}
