// Which browsers there are, as Rust last said: the one place the overview,
// the sidebar and the grid's browser cells read them from.

import type { BrowsersPage } from '../bindings/BrowsersPage';
import type { PageBrowser } from '../bindings/PageBrowser';

let page: BrowsersPage = { browsers: [] };

export function setBrowsers(event: BrowsersPage): void {
  page = event;
}

export function browsers(): readonly PageBrowser[] {
  return page.browsers;
}

export function byPane(pane: number | null): PageBrowser | null {
  return page.browsers.find((b) => b.pane === pane) ?? null;
}
